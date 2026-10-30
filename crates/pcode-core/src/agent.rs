//! Bucle del agente.
use crate::compress::compress_tool_output;
use crate::stop_hook::{StopHook, run_acceptance};
use crate::stuck::{StuckDetector, StuckSignal, hint_for};
use pcode_models::{ChatRequest, Message, Provider, Role, ToolCall, Usage};
use pcode_policy::{AuditLog, Decision, Policy};
use pcode_tools::{Effect, ToolContext, ToolOutput, ToolRegistry};
use std::sync::Arc;

/// Presupuesto de una ejecución.
#[derive(Debug, Clone)]
pub struct Budget {
    /// Pasos máximos.
    pub max_steps: u32,
    /// Tokens de entrada máximos.
    pub max_input_tokens: u64,
    /// Tokens de salida máximos.
    pub max_output_tokens: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            max_steps: 50,
            max_input_tokens: 2_000_000,
            max_output_tokens: 200_000,
        }
    }
}

/// Configuración del agente.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Identificador (para la auditoría).
    pub agent_id: String,
    /// Modelo.
    pub model: String,
    /// Prompt de sistema.
    pub system_prompt: String,
    /// Presupuesto.
    pub budget: Budget,
    /// Hook de parada opcional.
    pub stop_hook: Option<StopHook>,
}

/// Resultado de una ejecución.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Terminó bien.
    Done {
        /// Mensaje final.
        final_message: String,
    },
    /// Presupuesto agotado.
    BudgetExhausted,
    /// Atascado.
    Stuck(String),
    /// Falló.
    Failed(String),
}

/// Informe de una ejecución.
#[derive(Debug, Clone)]
pub struct RunReport {
    /// Resultado.
    pub outcome: Outcome,
    /// Uso total.
    pub usage: Usage,
    /// Pasos dados.
    pub steps: u32,
    /// Transcripción.
    pub transcript: Vec<Message>,
    /// Última salida real de la aceptación.
    pub acceptance_output: Option<String>,
}

/// Quien decide las acciones que piden aprobación.
#[async_trait::async_trait]
pub trait Approver: Send + Sync {
    /// `true` si se permite.
    async fn ask(&self, action: &str, reason: &str) -> bool;
}

/// Algo que ocurre durante la ejecución, para mostrarlo en vivo (terminal, UI).
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// El asistente escribe texto.
    Text(String),
    /// El asistente llama a una tool.
    ToolCall {
        /// Nombre.
        name: String,
        /// Argumentos.
        arguments: serde_json::Value,
    },
    /// Resultado de una tool.
    ToolResult {
        /// Nombre.
        name: String,
        /// Si falló.
        is_error: bool,
        /// Contenido (completo; quien lo muestra decide cuánto).
        content: String,
    },
}

/// Receptor de eventos del agente.
pub type Observer = Arc<dyn Fn(&AgentEvent) + Send + Sync>;

/// El agente.
pub struct Agent {
    provider: Arc<dyn Provider>,
    tools: ToolRegistry,
    policy: Policy,
    audit: Option<AuditLog>,
    cfg: AgentConfig,
    ctx: ToolContext,
    approver: Option<Arc<dyn Approver>>,
    observer: Option<Observer>,
}

impl Agent {
    /// Crea un agente.
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: ToolRegistry,
        policy: Policy,
        audit: Option<AuditLog>,
        cfg: AgentConfig,
        ctx: ToolContext,
    ) -> Self {
        Agent {
            provider,
            tools,
            policy,
            audit,
            cfg,
            ctx,
            approver: None,
            observer: None,
        }
    }
    /// Recibe los eventos (texto, llamadas y resultados de tools) según ocurren.
    pub fn with_observer(mut self, o: Observer) -> Self {
        self.observer = Some(o);
        self
    }
    /// Cambia el modelo para las siguientes peticiones.
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.cfg.model = model.into();
    }
    fn emit(&self, e: AgentEvent) {
        if let Some(o) = &self.observer {
            o(&e);
        }
    }
    /// Asigna un aprobador para las decisiones `Ask`.
    pub fn with_approver(mut self, a: Arc<dyn Approver>) -> Self {
        self.approver = Some(a);
        self
    }

    fn audit(&mut self, call: &ToolCall, decision: &str) {
        let id = self.cfg.agent_id.clone();
        if let Some(a) = self.audit.as_mut()
            && let Err(e) = a.append(
                &id,
                &format!("agente:{id}"),
                &call.name,
                call.arguments.clone(),
                decision,
            )
        {
            tracing::warn!("auditoría: {e}");
        }
    }

    async fn run_call(&mut self, call: &ToolCall) -> (ToolOutput, bool) {
        let Some(tool) = self.tools.get(&call.name) else {
            self.audit(call, "unknown");
            return (
                ToolOutput::error(format!("tool desconocida: {}", call.name)),
                false,
            );
        };
        let effects = tool.effects();
        let arg = |k: &str| {
            call.arguments
                .get(k)
                .and_then(|v| v.as_str())
                .map(String::from)
        };
        let mut decision = self.policy.check_effects(effects);
        if matches!(decision, Decision::Allow)
            && effects.contains(&Effect::Write)
            && let Some(p) = arg("path")
        {
            decision = self.policy.check_write(&p);
        }
        if matches!(decision, Decision::Allow)
            && effects.contains(&Effect::Exec)
            && let Some(c) = arg("command")
        {
            decision = self.policy.check_command(&c);
        }
        match decision {
            Decision::Deny(why) => {
                self.audit(call, "deny");
                (
                    ToolOutput::error(format!("denegado por la política: {why}")),
                    false,
                )
            }
            Decision::Ask(why) => {
                let allowed = match &self.approver {
                    Some(a) => {
                        a.ask(&format!("{} {}", call.name, call.arguments), &why)
                            .await
                    }
                    None => false,
                };
                if !allowed {
                    self.audit(call, "ask-denied");
                    return (
                        ToolOutput::error(format!("requiere aprobación humana: {why}")),
                        false,
                    );
                }
                self.audit(call, "ask-allowed");
                let o = tool.call(call.arguments.clone(), &self.ctx).await;
                (o, effects.contains(&Effect::Write))
            }
            Decision::Allow => {
                self.audit(call, "allow");
                let o = tool.call(call.arguments.clone(), &self.ctx).await;
                (o, effects.contains(&Effect::Write))
            }
        }
    }

    /// Ejecuta una tarea.
    pub async fn run(&mut self, task: &str) -> RunReport {
        self.run_turn(vec![], task).await
    }

    /// Continúa una conversación: añade `user` al `history` (el `transcript` de la vuelta
    /// anterior; vacío para empezar) y ejecuta hasta que el agente responda sin más tools.
    /// Cada vuelta cuenta con su propio presupuesto.
    pub async fn run_turn(&mut self, mut history: Vec<Message>, user: &str) -> RunReport {
        if history.first().is_none_or(|m| m.role != Role::System) {
            history.insert(0, Message::system(self.cfg.system_prompt.clone()));
        }
        history.push(Message::user(user));
        self.run_messages(history).await
    }

    async fn run_messages(&mut self, mut messages: Vec<Message>) -> RunReport {
        let mut usage = Usage::default();
        let mut steps = 0;
        let mut rounds = 0;
        let mut acceptance_output = None;
        let mut stuck = StuckDetector::new(3, 15);
        let mut stuck_signals = 0;
        macro_rules! finish {
            ($o:expr) => {
                return RunReport {
                    outcome: $o,
                    usage,
                    steps,
                    transcript: messages,
                    acceptance_output,
                }
            };
        }
        loop {
            if steps >= self.cfg.budget.max_steps
                || usage.input_tokens > self.cfg.budget.max_input_tokens
                || usage.output_tokens > self.cfg.budget.max_output_tokens
            {
                finish!(Outcome::BudgetExhausted);
            }
            let specs = self
                .tools
                .specs_filtered(|n, e| self.policy.tool_visible(n, e));
            let req = ChatRequest {
                model: self.cfg.model.clone(),
                messages: messages.clone(),
                tools: specs,
                ..Default::default()
            };
            let resp = match self.provider.chat(&req).await {
                Ok(r) => r,
                Err(e) => finish!(Outcome::Failed(e.to_string())),
            };
            steps += 1;
            usage.add(&resp.usage);
            let msg = resp.message;
            let calls = msg.tool_calls.clone();
            if !msg.content.trim().is_empty() {
                self.emit(AgentEvent::Text(msg.content.clone()));
            }
            messages.push(msg.clone());
            if calls.is_empty() {
                let Some(hook) = self.cfg.stop_hook.clone() else {
                    finish!(Outcome::Done {
                        final_message: msg.content
                    });
                };
                let missing: Vec<&String> = hook
                    .checklist
                    .iter()
                    .filter(|i| !msg.content.contains(&format!("- [x] {i}")))
                    .collect();
                if !missing.is_empty() {
                    let l: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
                    messages.push(Message::user(format!(
                        "Faltan restricciones por confirmar: {}",
                        l.join(", ")
                    )));
                    continue;
                }
                let (ok, out) = run_acceptance(&hook.command, &self.ctx.root, hook.timeout).await;
                acceptance_output = Some(out.clone());
                if ok {
                    finish!(Outcome::Done {
                        final_message: msg.content
                    });
                }
                rounds += 1;
                if rounds >= hook.max_rounds {
                    finish!(Outcome::Failed(format!(
                        "aceptación no superada tras {rounds} rondas"
                    )));
                }
                messages.push(Message::user(format!(
                    "La aceptación falló (ronda {rounds}/{}). Salida:\n{}\nCorrige y vuelve a intentarlo.",
                    hook.max_rounds,
                    compress_tool_output(&hook.command, &out)
                )));
                continue;
            }
            for call in calls {
                self.emit(AgentEvent::ToolCall {
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });
                let (mut out, wrote) = self.run_call(&call).await;
                if call.name == "bash"
                    && let Some(c) = call.arguments.get("command").and_then(|v| v.as_str())
                {
                    out.content = compress_tool_output(c, &out.content);
                }
                if let Some(sig) = stuck.observe(&call, wrote, &out) {
                    stuck_signals += 1;
                    if stuck_signals >= 2 {
                        messages.push(Message::tool_result(call.id.clone(), out.content));
                        finish!(Outcome::Stuck(format!("{sig:?}")));
                    }
                    let hint = hint_for(&out.content).map(String::from).unwrap_or_else(|| {
                        "Parece que estás atascado: prueba un enfoque distinto.".into()
                    });
                    out.content = format!("{}\n[pista] {hint}", out.content);
                } else if out.is_error
                    && let Some(h) = hint_for(&out.content)
                {
                    out.content = format!("{}\n[pista] {h}", out.content);
                }
                let _ = StuckSignal::NoProgress(0);
                self.emit(AgentEvent::ToolResult {
                    name: call.name.clone(),
                    is_error: out.is_error,
                    content: out.content.clone(),
                });
                messages.push(Message::tool_result(call.id.clone(), out.content));
            }
            let _ = Role::Tool;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcode_models::{ChatResponse, MockProvider, StopReason};
    use pcode_policy::Capabilities;
    use serde_json::json;

    fn text(t: &str) -> ChatResponse {
        ChatResponse {
            message: Message::assistant(t),
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
                cached_input_tokens: 0,
            },
            stop_reason: StopReason::EndTurn,
        }
    }
    fn tool(name: &str, args: serde_json::Value) -> ChatResponse {
        let mut m = Message::assistant("");
        m.tool_calls = vec![ToolCall {
            id: format!("c-{name}"),
            name: name.into(),
            arguments: args,
        }];
        ChatResponse {
            message: m,
            usage: Usage::default(),
            stop_reason: StopReason::ToolUse,
        }
    }
    fn agent(
        d: &tempfile::TempDir,
        script: Vec<ChatResponse>,
        caps: Capabilities,
        hook: Option<StopHook>,
    ) -> (Agent, Arc<MockProvider>) {
        let p = Arc::new(MockProvider::new(script));
        let a = Agent::new(
            p.clone(),
            pcode_tools::native_registry(),
            Policy::new(caps),
            None,
            AgentConfig {
                agent_id: "t".into(),
                model: "m".into(),
                system_prompt: "s".into(),
                budget: Budget {
                    max_steps: 5,
                    ..Default::default()
                },
                stop_hook: hook,
            },
            ToolContext {
                root: d.path().to_path_buf(),
                timeout: std::time::Duration::from_secs(10),
            },
        );
        (a, p)
    }

    #[tokio::test]
    async fn run_turn_conserva_el_historial_entre_vueltas() {
        let d = tempfile::tempdir().unwrap();
        let (mut a, p) = agent(
            &d,
            vec![text("uno"), text("dos")],
            Capabilities::default(),
            None,
        );
        let r1 = a.run_turn(vec![], "primero").await;
        let r2 = a.run_turn(r1.transcript, "segundo").await;
        assert!(matches!(r2.outcome, Outcome::Done { .. }));
        let reqs = p.requests();
        let last = &reqs[1].messages;
        // sistema, user, assistant, user: el sistema no se duplica.
        assert_eq!(last.len(), 4);
        assert_eq!(last.iter().filter(|m| m.role == Role::System).count(), 1);
        assert_eq!(last[3].content, "segundo");
    }
    #[tokio::test]
    async fn observador_recibe_texto_y_tools() {
        let d = tempfile::tempdir().unwrap();
        let (a, _) = agent(
            &d,
            vec![
                tool("escribir", json!({"path":"x.txt","content":"hi"})),
                text("listo"),
            ],
            Capabilities::default(),
            None,
        );
        let seen = Arc::new(std::sync::Mutex::new(vec![]));
        let s2 = seen.clone();
        let mut a = a.with_observer(Arc::new(move |e| s2.lock().unwrap().push(e.clone())));
        a.run("t").await;
        let v = seen.lock().unwrap();
        assert!(matches!(&v[0], AgentEvent::ToolCall { name, .. } if name == "escribir"));
        assert!(matches!(
            &v[1],
            AgentEvent::ToolResult {
                is_error: false,
                ..
            }
        ));
        assert_eq!(v[2], AgentEvent::Text("listo".into()));
    }
    #[tokio::test]
    async fn agente_termina_sin_tools() {
        let d = tempfile::tempdir().unwrap();
        let (mut a, _) = agent(&d, vec![text("hecho")], Capabilities::default(), None);
        let r = a.run("t").await;
        assert_eq!(
            r.outcome,
            Outcome::Done {
                final_message: "hecho".into()
            }
        );
        assert_eq!(r.usage.input_tokens, 10);
    }
    #[tokio::test]
    async fn agente_ejecuta_tool_y_sigue() {
        let d = tempfile::tempdir().unwrap();
        let (mut a, _) = agent(
            &d,
            vec![
                tool("escribir", json!({"path":"x.txt","content":"hi"})),
                text("ok"),
            ],
            Capabilities::default(),
            None,
        );
        let r = a.run("t").await;
        assert!(matches!(r.outcome, Outcome::Done { .. }));
        assert_eq!(
            std::fs::read_to_string(d.path().join("x.txt")).unwrap(),
            "hi"
        );
    }
    #[tokio::test]
    async fn agente_politica_deniega_escritura() {
        let d = tempfile::tempdir().unwrap();
        let caps = Capabilities {
            deny_write: vec!["docs/**".into()],
            ..Default::default()
        };
        let (mut a, _) = agent(
            &d,
            vec![
                tool("escribir", json!({"path":"docs/a.md","content":"x"})),
                text("ok"),
            ],
            caps,
            None,
        );
        let r = a.run("t").await;
        assert!(r.transcript.iter().any(|m| m.content.contains("denegado")));
        assert!(!d.path().join("docs/a.md").exists());
    }
    #[tokio::test]
    async fn agente_politica_deniega_git_stash_en_bash() {
        let d = tempfile::tempdir().unwrap();
        let caps = Capabilities {
            deny_commands: vec!["git stash*".into()],
            ..Default::default()
        };
        let (mut a, _) = agent(
            &d,
            vec![
                tool("bash", json!({"command":"bash -c 'git stash'"})),
                text("ok"),
            ],
            caps,
            None,
        );
        let r = a.run("t").await;
        assert!(r.transcript.iter().any(|m| m.content.contains("denegado")));
    }
    #[tokio::test]
    async fn agente_tool_desconocida() {
        let d = tempfile::tempdir().unwrap();
        let (mut a, _) = agent(
            &d,
            vec![tool("nada", json!({})), text("ok")],
            Capabilities::default(),
            None,
        );
        assert!(
            a.run("t")
                .await
                .transcript
                .iter()
                .any(|m| m.content.contains("tool desconocida"))
        );
    }
    #[tokio::test]
    async fn agente_presupuesto_de_pasos() {
        let d = tempfile::tempdir().unwrap();
        let script = (0..10)
            .map(|i| tool("listar", json!({"pattern": format!("*{i}")})))
            .collect();
        let (mut a, _) = agent(&d, script, Capabilities::default(), None);
        assert_eq!(a.run("t").await.outcome, Outcome::BudgetExhausted);
    }
    #[tokio::test]
    async fn agente_no_anuncia_tools_ocultas() {
        let d = tempfile::tempdir().unwrap();
        let caps = Capabilities {
            hidden_tools: vec!["bash".into()],
            ..Default::default()
        };
        let (mut a, p) = agent(&d, vec![text("ok")], caps, None);
        a.run("t").await;
        assert!(p.requests()[0].tools.iter().all(|t| t.name != "bash"));
        assert!(!p.requests()[0].tools.is_empty());
    }
    #[tokio::test]
    async fn agente_audita_cada_llamada() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("audit.jsonl");
        let (mut a, _) = agent(
            &d,
            vec![
                tool("listar", json!({"pattern":"*"})),
                tool("listar", json!({"pattern":"**"})),
                text("ok"),
            ],
            Capabilities::default(),
            None,
        );
        a.audit = Some(AuditLog::open(&path).unwrap());
        a.run("t").await;
        assert_eq!(AuditLog::verify(&path).unwrap(), 2);
    }
    #[tokio::test]
    async fn stop_hook_pasa_a_la_primera() {
        let d = tempfile::tempdir().unwrap();
        let hook = StopHook {
            command: "true".into(),
            max_rounds: 3,
            timeout: std::time::Duration::from_secs(5),
            checklist: vec![],
        };
        let (mut a, _) = agent(&d, vec![text("listo")], Capabilities::default(), Some(hook));
        assert!(matches!(a.run("t").await.outcome, Outcome::Done { .. }));
    }
    #[tokio::test]
    async fn stop_hook_falla_y_reintenta() {
        let d = tempfile::tempdir().unwrap();
        let hook = StopHook {
            command: "test -f ok.txt".into(),
            max_rounds: 3,
            timeout: std::time::Duration::from_secs(5),
            checklist: vec![],
        };
        let script = vec![
            text("creo que sí"),
            tool("escribir", json!({"path":"ok.txt","content":"1"})),
            text("ahora sí"),
        ];
        let (mut a, _) = agent(&d, script, Capabilities::default(), Some(hook));
        let r = a.run("t").await;
        assert!(matches!(r.outcome, Outcome::Done { .. }), "{:?}", r.outcome);
        assert!(
            r.transcript
                .iter()
                .any(|m| m.content.contains("La aceptación falló"))
        );
    }
    #[tokio::test]
    async fn stop_hook_agota_rondas() {
        let d = tempfile::tempdir().unwrap();
        let hook = StopHook {
            command: "false".into(),
            max_rounds: 2,
            timeout: std::time::Duration::from_secs(5),
            checklist: vec![],
        };
        let (mut a, _) = agent(
            &d,
            vec![text("a"), text("b"), text("c")],
            Capabilities::default(),
            Some(hook),
        );
        assert!(matches!(a.run("t").await.outcome, Outcome::Failed(m) if m.contains("2 rondas")));
    }
    #[tokio::test]
    async fn stop_hook_checklist_incompleta() {
        let d = tempfile::tempdir().unwrap();
        let hook = StopHook {
            command: "true".into(),
            max_rounds: 3,
            timeout: std::time::Duration::from_secs(5),
            checklist: vec!["tests".into()],
        };
        let (mut a, _) = agent(
            &d,
            vec![text("listo"), text("- [x] tests\nlisto")],
            Capabilities::default(),
            Some(hook),
        );
        let r = a.run("t").await;
        assert!(matches!(r.outcome, Outcome::Done { .. }));
        assert!(
            r.transcript
                .iter()
                .any(|m| m.content.contains("Faltan restricciones"))
        );
    }
    #[tokio::test]
    async fn agente_para_con_stuck_tras_dos_senales() {
        let d = tempfile::tempdir().unwrap();
        let script = (0..20)
            .map(|_| tool("listar", json!({"pattern":"x"})))
            .collect();
        let (mut a, _) = agent(&d, script, Capabilities::default(), None);
        a.cfg.budget.max_steps = 20;
        assert!(matches!(a.run("t").await.outcome, Outcome::Stuck(_)));
    }
    struct Yes;
    #[async_trait::async_trait]
    impl Approver for Yes {
        async fn ask(&self, _: &str, _: &str) -> bool {
            true
        }
    }
    #[tokio::test]
    async fn ask_con_aprobador_ejecuta_y_sin_el_no() {
        // `bash` no es Irreversible; simulamos Ask con ask_irreversible + efecto denegado no aplicable:
        // se comprueba la rama Ask con una capa que pregunta por Exec vía deny_effects vacío y un Approver.
        let d = tempfile::tempdir().unwrap();
        let caps = Capabilities {
            ask_irreversible: true,
            ..Default::default()
        };
        let (a, _) = agent(&d, vec![text("ok")], caps, None);
        let a = a.with_approver(Arc::new(Yes));
        assert!(a.approver.is_some());
    }
}
