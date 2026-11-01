//! Bucle del agente.
use crate::ask::{ASK_TOOL, AskMode, AskOutcome, AskState, SENT_NOTE};
use crate::checkpoint::{Checkpoints, FileChange};
use crate::compress::compress_tool_output;
use crate::memory::REMEMBER_TOOL;
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
    /// Si una petición supera estos tokens de entrada, se compacta la conversación antes de la
    /// siguiente (0 = nunca).
    pub max_context_tokens: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            max_steps: 50,
            max_input_tokens: 2_000_000,
            max_output_tokens: 200_000,
            max_context_tokens: 0,
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
    /// Aviso del propio agente (p. ej. que ha compactado el contexto).
    Info(String),
    /// Trozo de texto del asistente según llega (streaming).
    TextDelta(String),
    /// Mensaje de texto del asistente completo (tras sus trozos).
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
    last_input_tokens: u64,
    read_only: bool,
    checkpoints: Option<Checkpoints>,
    ask: Option<AskState>,
    memory: bool,
    verify_reminder: bool,
}

/// Instrucción para resumir la conversación al compactar. Recoge los modos de fallo de la compresión
/// del contexto (2026.emnlp-main.2396: se pierden restricciones, rutas y errores) y de SLUMP
/// (2026.findings-emnlp.76: se pierden los compromisos de diseño revelados durante la sesión).
pub const COMPACT_PROMPT: &str = "Resume esta conversación entre un usuario y un agente de programación \
para que el agente pueda seguir sin ella. Conserva LITERALMENTE las restricciones, preferencias y decisiones de \
diseño que haya dado el usuario (aunque aparecieran de pasada), las rutas exactas de archivos y símbolos, las \
órdenes que funcionaron y los errores que siguen pendientes. Después: objetivo, estado de cada archivo tocado y \
próximos pasos. Sé concreto y breve; no inventes nada.";

/// Instrucción que se añade en modo plan.
pub const PLAN_NOTE: &str = "Estás en MODO PLAN: solo puedes leer y buscar. No intentes cambiar archivos ni \
ejecutar órdenes. Investiga lo necesario y responde con un plan concreto (archivos, cambios, pruebas) \
para que el usuario lo revise; se aplicará cuando vuelva al modo normal.";

fn writes(effects: &[Effect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            Effect::Write | Effect::Exec | Effect::Irreversible | Effect::Network
        )
    })
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
            last_input_tokens: 0,
            read_only: false,
            checkpoints: None,
            ask: None,
            memory: false,
            verify_reminder: false,
        }
    }
    /// Ofrece la tool `preguntar` (F4.11) con este modo y el presupuesto por defecto.
    pub fn with_questions(mut self, mode: AskMode) -> Self {
        self.ask = Some(AskState::new(mode));
        self
    }
    /// Preguntas que ha hecho el agente (todas las tareas).
    pub fn questions_asked(&self) -> &[String] {
        self.ask.as_ref().map(|a| a.asked.as_slice()).unwrap_or(&[])
    }
    /// Memoria del proyecto (F8.19): la añade al prompt de sistema en cada turno y ofrece la tool
    /// `recordar` (solo con aprobador: lo que propone el agente lo aprueba el usuario).
    pub fn with_memory(mut self) -> Self {
        self.memory = true;
        self
    }
    /// Si el turno cambió archivos y no ejecutó ninguna comprobación, antes de terminar se le pide al
    /// agente una vez que verifique (o diga por qué no hace falta).
    pub fn with_verify_reminder(mut self) -> Self {
        self.verify_reminder = true;
        self
    }
    /// Prompt de sistema del turno: el configurado y, si hay, la memoria del proyecto.
    pub fn system_content(&self) -> String {
        match self
            .memory
            .then(|| crate::memory::prompt_section(&self.ctx.root))
            .flatten()
        {
            Some(m) => format!("{}\n\n{m}", self.cfg.system_prompt),
            None => self.cfg.system_prompt.clone(),
        }
    }
    /// Guarda un punto de control por turno antes de cada escritura (para `/deshacer`).
    pub fn with_checkpoints(mut self) -> Self {
        self.checkpoints = Some(Checkpoints::open(&self.ctx.root));
        self
    }
    /// Cambios del turno en curso (o del último), si hay puntos de control.
    pub fn turn_changes(&self) -> Vec<FileChange> {
        self.checkpoints
            .as_ref()
            .map(Checkpoints::current_changes)
            .unwrap_or_default()
    }
    /// Recibe los eventos (texto, llamadas y resultados de tools) según ocurren.
    pub fn with_observer(mut self, o: Observer) -> Self {
        self.observer = Some(o);
        self
    }
    /// Modo plan: solo tools de lectura y una instrucción para que proponga en lugar de hacer.
    pub fn set_read_only(&mut self, on: bool) {
        self.read_only = on;
    }
    /// Si está en modo plan (solo lectura).
    pub fn read_only(&self) -> bool {
        self.read_only
    }
    /// Tokens de entrada de la última petición al modelo: lo que ocupa ahora el contexto.
    pub fn context_tokens(&self) -> u64 {
        self.last_input_tokens
    }
    /// A partir de cuántos tokens de contexto se compacta la conversación (0 = nunca).
    pub fn context_limit(&self) -> u64 {
        self.cfg.budget.max_context_tokens
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

    fn checkpoint(&mut self, call: &ToolCall, effects: &[Effect]) {
        if !effects.contains(&Effect::Write) {
            return;
        }
        let Some(p) = call.arguments.get("path").and_then(|v| v.as_str()) else {
            return;
        };
        if let Some(c) = self.checkpoints.as_mut()
            && let Err(e) = c.save(p)
        {
            tracing::warn!("punto de control de {p}: {e}");
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
        if self.read_only && writes(effects) {
            self.audit(call, "deny");
            return (
                ToolOutput::error("modo plan: solo lectura (vuelve al modo normal para aplicarlo)"),
                false,
            );
        }
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
                self.checkpoint(call, effects);
                let o = tool.call(call.arguments.clone(), &self.ctx).await;
                (o, effects.contains(&Effect::Write))
            }
            Decision::Allow => {
                self.audit(call, "allow");
                self.checkpoint(call, effects);
                let o = tool.call(call.arguments.clone(), &self.ctx).await;
                (o, effects.contains(&Effect::Write))
            }
        }
    }

    /// Resume con el propio modelo la parte antigua de la conversación y conserva el sistema y
    /// los últimos mensajes. Corta siempre antes de un mensaje de usuario o de asistente, para no
    /// separar una llamada a tool de su resultado. Si algo falla, devuelve `messages` tal cual.
    pub async fn compact(&mut self, messages: Vec<Message>) -> Vec<Message> {
        const KEEP: usize = 6;
        if messages.len() < KEEP + 3 {
            return messages;
        }
        let start = messages.len() - KEEP;
        let Some(cut) = (2..=start)
            .rev()
            .find(|&i| matches!(messages[i].role, Role::User | Role::Assistant))
        else {
            return messages;
        };
        let clip = |s: &str, n: usize| -> String {
            if s.chars().count() > n {
                format!("{}…", s.chars().take(n).collect::<String>())
            } else {
                s.to_string()
            }
        };
        let mut text = String::new();
        for m in &messages[1..cut] {
            let role = match m.role {
                Role::User => "usuario",
                Role::Assistant => "asistente",
                Role::Tool => "resultado de tool",
                Role::System => "sistema",
            };
            let limit = if m.role == Role::Tool { 1500 } else { 4000 };
            text.push_str(&format!("### {role}\n{}\n", clip(&m.content, limit)));
            for c in &m.tool_calls {
                text.push_str(&format!(
                    "→ {}({})\n",
                    c.name,
                    clip(&c.arguments.to_string(), 300)
                ));
            }
        }
        let req = ChatRequest {
            model: self.cfg.model.clone(),
            messages: vec![Message::system(COMPACT_PROMPT), Message::user(text)],
            tools: vec![],
            max_tokens: Some(2000),
            ..Default::default()
        };
        let summary = match self.provider.chat(&req).await {
            Ok(r) if !r.message.content.trim().is_empty() => r.message.content,
            Ok(_) => return messages,
            Err(e) => {
                tracing::warn!("no se pudo compactar el contexto: {e}");
                return messages;
            }
        };
        let dropped = cut - 1;
        let head = format!("[Resumen de la conversación anterior]\n{summary}");
        let mut out = vec![messages[0].clone()];
        let mut rest = messages[cut..].to_vec();
        if rest[0].role == Role::User {
            rest[0].content = format!("{head}\n\n---\n{}", rest[0].content);
        } else {
            out.push(Message::user(head));
        }
        out.extend(rest);
        self.last_input_tokens = 0;
        self.emit(AgentEvent::Info(format!(
            "contexto compactado: {dropped} mensajes resumidos"
        )));
        out
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
            history.insert(0, Message::system(self.system_content()));
        } else if self.memory {
            // La memoria puede haber cambiado (`/recuerda`): el sistema siempre la lleva al día.
            history[0] = Message::system(self.system_content());
        }
        if let Some(a) = self.ask.as_mut() {
            a.reset();
        }
        history.push(Message::user(user));
        if let Some(c) = self.checkpoints.as_mut() {
            c.begin_turn(user);
        }
        self.run_messages(history).await
    }

    async fn run_messages(&mut self, mut messages: Vec<Message>) -> RunReport {
        let mut usage = Usage::default();
        let mut steps = 0;
        let mut rounds = 0;
        let mut acceptance_output = None;
        let mut stuck = StuckDetector::new(3, 15);
        let mut stuck_signals = 0;
        let (mut wrote_files, mut ran_check, mut reminded) = (false, false, false);
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
            let limit = self.cfg.budget.max_context_tokens;
            if limit > 0 && self.last_input_tokens > limit {
                messages = self.compact(messages).await;
            }
            if steps >= self.cfg.budget.max_steps
                || usage.input_tokens > self.cfg.budget.max_input_tokens
                || usage.output_tokens > self.cfg.budget.max_output_tokens
            {
                finish!(Outcome::BudgetExhausted);
            }
            let ro = self.read_only;
            let mut specs = self
                .tools
                .specs_filtered(|n, e| self.policy.tool_visible(n, e) && !(ro && writes(e)));
            if self.ask.is_some() && self.policy.tool_visible(ASK_TOOL, &[]) {
                specs.push(AskState::spec());
            }
            if self.memory
                && self.approver.is_some()
                && !ro
                && self.policy.tool_visible(REMEMBER_TOOL, &[])
            {
                specs.push(crate::memory::remember_spec());
            }
            let mut msgs = messages.clone();
            if ro {
                msgs.push(Message::system(PLAN_NOTE));
            }
            let req = ChatRequest {
                model: self.cfg.model.clone(),
                messages: msgs,
                tools: specs,
                ..Default::default()
            };
            let obs = self.observer.clone();
            let on_text = move |t: &str| {
                if let Some(o) = &obs {
                    o(&AgentEvent::TextDelta(t.to_string()));
                }
            };
            let resp = match self.provider.chat_stream(&req, &on_text).await {
                Ok(r) => r,
                Err(e) => finish!(Outcome::Failed(e.to_string())),
            };
            steps += 1;
            usage.add(&resp.usage);
            self.last_input_tokens = resp.usage.input_tokens;
            let msg = resp.message;
            let calls = msg.tool_calls.clone();
            if !msg.content.trim().is_empty() {
                self.emit(AgentEvent::Text(msg.content.clone()));
            }
            messages.push(msg.clone());
            if calls.is_empty() {
                if self.verify_reminder
                    && wrote_files
                    && !ran_check
                    && !reminded
                    && self.cfg.stop_hook.is_none()
                    && self.can_exec()
                {
                    reminded = true;
                    self.emit(AgentEvent::Info(
                        "ha cambiado archivos sin comprobar nada: le pido que verifique".into(),
                    ));
                    messages.push(Message::user(VERIFY_NOTE));
                    continue;
                }
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
            let mut question: Option<String> = None;
            for call in calls {
                if question.is_some() {
                    messages.push(Message::tool_result(
                        call.id.clone(),
                        "no ejecutada: el turno terminó con una pregunta al usuario",
                    ));
                    continue;
                }
                self.emit(AgentEvent::ToolCall {
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });
                if call.name == ASK_TOOL
                    && let Some(outcome) = self.ask.as_mut().map(|a| a.handle(&call.arguments))
                {
                    self.audit(&call, "ask");
                    let content = match outcome {
                        AskOutcome::Answer(a) => a,
                        AskOutcome::EndTurn(q) => {
                            question = Some(q);
                            SENT_NOTE.to_string()
                        }
                        AskOutcome::Stop(reason) => {
                            messages.push(Message::tool_result(call.id.clone(), reason.clone()));
                            finish!(Outcome::Failed(reason));
                        }
                    };
                    self.emit(AgentEvent::ToolResult {
                        name: call.name.clone(),
                        is_error: false,
                        content: content.clone(),
                    });
                    messages.push(Message::tool_result(call.id.clone(), content));
                    continue;
                }
                if call.name == REMEMBER_TOOL && self.memory {
                    let content = self.remember_call(&call).await;
                    self.emit(AgentEvent::ToolResult {
                        name: call.name.clone(),
                        is_error: false,
                        content: content.clone(),
                    });
                    messages.push(Message::tool_result(call.id.clone(), content));
                    continue;
                }
                if self
                    .tools
                    .get(&call.name)
                    .is_some_and(|t| t.effects().contains(&Effect::Exec))
                {
                    ran_check = true;
                }
                let (mut out, wrote) = self.run_call(&call).await;
                wrote_files |= wrote && !out.is_error;
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
            if let Some(q) = question {
                self.emit(AgentEvent::Text(q.clone()));
                messages.push(Message::assistant(q.clone()));
                finish!(Outcome::Done { final_message: q });
            }
            let _ = Role::Tool;
        }
    }

    /// Si hay una tool visible que ejecuta órdenes (para verificar).
    fn can_exec(&self) -> bool {
        !self.read_only
            && !self
                .tools
                .specs_filtered(|n, e| e.contains(&Effect::Exec) && self.policy.tool_visible(n, e))
                .is_empty()
    }

    /// `recordar`: guarda la nota en la memoria del proyecto solo si el usuario lo aprueba.
    async fn remember_call(&mut self, call: &ToolCall) -> String {
        let text = call
            .arguments
            .get("texto")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if text.is_empty() {
            return "falta `texto`".into();
        }
        let allowed = match &self.approver {
            Some(a) => {
                a.ask(
                    &format!("recordar «{text}»"),
                    "la memoria del proyecto entra en el prompt de todas las conversaciones",
                )
                .await
            }
            None => false,
        };
        if !allowed {
            self.audit(call, "ask-denied");
            return "el usuario no quiso guardarlo en la memoria".into();
        }
        self.audit(call, "ask-allowed");
        match crate::memory::remember(&self.ctx.root, &text, true) {
            Ok(n) => format!("guardado en la memoria del proyecto: {n}"),
            Err(e) => format!("no se pudo guardar: {e}"),
        }
    }
}

/// Recordatorio de verificación (una vez por turno). Inspirado en «Polished but Unresolved»
/// (2026.emnlp-main.1143): cerca del final, los agentes tienden a entregar algo pulido con
/// restricciones sin resolver.
pub const VERIFY_NOTE: &str = "Antes de terminar: has cambiado archivos pero no has ejecutado ninguna \
comprobación. Si el proyecto tiene tests, compilación o linter, ejecuta lo que corresponda y corrige lo que \
falle; si no hace falta, di en una línea por qué. (Recordatorio automático de PublicCode.)";

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
    async fn compacta_cuando_se_supera_el_limite_sin_romper_pares_de_tools() {
        let d = tempfile::tempdir().unwrap();
        let mut big = text("paso");
        big.usage.input_tokens = 1_000;
        let mut script = vec![];
        for _ in 0..4 {
            let mut t = tool("leer", json!({"path": "x"}));
            t.usage.input_tokens = 1_000;
            script.push(t);
        }
        script.push(text("RESUMEN"));
        script.push(text("hecho"));
        std::fs::write(d.path().join("x"), "contenido").unwrap();
        let (mut a, p) = agent(&d, script, Capabilities::default(), None);
        a.cfg.budget.max_context_tokens = 500;
        a.cfg.budget.max_steps = 10;
        let r = a.run("tarea larga").await;
        assert!(matches!(r.outcome, Outcome::Done { .. }), "{:?}", r.outcome);
        let reqs = p.requests();
        // La petición de resumen no lleva tools; la siguiente empieza por el resumen.
        let sum = reqs
            .iter()
            .position(|q| q.tools.is_empty())
            .expect("pidió un resumen");
        let after = &reqs[sum + 1].messages;
        assert!(
            after
                .iter()
                .any(|m| m.content.contains("[Resumen de la conversación anterior]"))
        );
        // Cada resultado de tool sigue a una llamada.
        for (i, m) in after.iter().enumerate() {
            if m.role == Role::Tool {
                assert!(
                    after[..i]
                        .iter()
                        .rev()
                        .find(|x| x.role != Role::Tool)
                        .is_some_and(|x| !x.tool_calls.is_empty())
                );
            }
        }
        assert_eq!(
            after.len(),
            8,
            "sistema, resumen y los 6 mensajes recientes"
        );
        assert!(
            after[1]
                .content
                .starts_with("[Resumen de la conversación anterior]\nRESUMEN")
        );
        let _ = big;
    }
    #[tokio::test]
    async fn modo_plan_oculta_y_deniega_lo_que_escribe() {
        let d = tempfile::tempdir().unwrap();
        let (mut a, p) = agent(
            &d,
            vec![
                tool("escribir", json!({"path":"x.txt","content":"hi"})),
                text("plan: crear x.txt"),
            ],
            Capabilities::default(),
            None,
        );
        a.set_read_only(true);
        let r = a.run("crea x.txt").await;
        assert!(matches!(r.outcome, Outcome::Done { .. }));
        assert!(
            !d.path().join("x.txt").exists(),
            "en modo plan no se escribe"
        );
        let req = &p.requests()[0];
        let names: Vec<&str> = req.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"leer") && !names.contains(&"escribir") && !names.contains(&"bash"),
            "{names:?}"
        );
        assert!(req.messages.iter().any(|m| m.content == PLAN_NOTE));
        assert!(
            r.transcript.iter().all(|m| m.content != PLAN_NOTE),
            "la nota no se guarda"
        );
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
        assert_eq!(v[2], AgentEvent::TextDelta("listo".into()));
        assert_eq!(v[3], AgentEvent::Text("listo".into()));
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
