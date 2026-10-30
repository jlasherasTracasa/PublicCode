//! Tipos comunes a todos los proveedores.
use serde::{Deserialize, Serialize};

/// Rol de un mensaje.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Sistema.
    System,
    /// Usuario.
    User,
    /// Asistente.
    Assistant,
    /// Resultado de tool.
    Tool,
}

/// Llamada a una tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Identificador.
    pub id: String,
    /// Nombre de la tool.
    pub name: String,
    /// Argumentos.
    pub arguments: serde_json::Value,
}

/// Mensaje de una conversación.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Rol.
    pub role: Role,
    /// Contenido.
    pub content: String,
    /// Llamadas a tools (asistente).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Id de la llamada a la que responde (tool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    fn new(role: Role, text: impl Into<String>) -> Self {
        Message {
            role,
            content: text.into(),
            tool_calls: vec![],
            tool_call_id: None,
        }
    }
    /// Mensaje de sistema.
    pub fn system(text: impl Into<String>) -> Self {
        Self::new(Role::System, text)
    }
    /// Mensaje de usuario.
    pub fn user(text: impl Into<String>) -> Self {
        Self::new(Role::User, text)
    }
    /// Mensaje del asistente.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new(Role::Assistant, text)
    }
    /// Resultado de una tool.
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        let mut m = Self::new(Role::Tool, content);
        m.tool_call_id = Some(call_id.into());
        m
    }
}

/// Descripción de una tool para el modelo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Nombre.
    pub name: String,
    /// Descripción.
    pub description: String,
    /// JSON Schema de los parámetros.
    pub parameters: serde_json::Value,
}

/// Petición de chat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChatRequest {
    /// Modelo.
    pub model: String,
    /// Mensajes.
    pub messages: Vec<Message>,
    /// Tools.
    pub tools: Vec<ToolSpec>,
    /// Máximo de tokens de salida.
    pub max_tokens: Option<u32>,
    /// Temperatura.
    pub temperature: Option<f32>,
}

/// Uso de tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Usage {
    /// Entrada.
    pub input_tokens: u64,
    /// Salida.
    pub output_tokens: u64,
    /// Entrada servida desde caché.
    pub cached_input_tokens: u64,
}

impl Usage {
    /// Suma otro uso.
    pub fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
    }
}

/// Motivo de parada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    /// Fin de turno.
    EndTurn,
    /// Pide usar tools.
    ToolUse,
    /// Límite de tokens.
    MaxTokens,
    /// Otro.
    Other(String),
}

/// Respuesta de chat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Mensaje del asistente.
    pub message: Message,
    /// Uso.
    pub usage: Usage,
    /// Motivo de parada.
    pub stop_reason: StopReason,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn message_constructores() {
        assert_eq!(Message::system("a").role, Role::System);
        assert_eq!(Message::user("a").role, Role::User);
        assert_eq!(Message::assistant("a").role, Role::Assistant);
        assert_eq!(Message::tool_result("1", "a").role, Role::Tool);
    }
    #[test]
    fn message_serializa_sin_campos_vacios() {
        let j = serde_json::to_string(&Message::user("hola")).unwrap();
        assert!(!j.contains("tool_calls") && !j.contains("tool_call_id"));
    }
    #[test]
    fn tool_result_lleva_id() {
        assert_eq!(
            Message::tool_result("x", "y").tool_call_id.as_deref(),
            Some("x")
        );
    }
    #[test]
    fn usage_add_suma_los_tres_campos() {
        let mut a = Usage {
            input_tokens: 1,
            output_tokens: 2,
            cached_input_tokens: 3,
        };
        a.add(&a.clone());
        assert_eq!(
            (a.input_tokens, a.output_tokens, a.cached_input_tokens),
            (2, 4, 6)
        );
    }
    #[test]
    fn chat_response_ida_y_vuelta_json() {
        let r = ChatResponse {
            message: Message::assistant("ok"),
            usage: Usage::default(),
            stop_reason: StopReason::EndTurn,
        };
        let back: ChatResponse = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(r, back);
    }
}
