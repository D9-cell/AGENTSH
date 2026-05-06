use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::LlmConfig;
use crate::tools::ToolSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct LlmClient {
    client: Client,
    base_url: String,
    model: String,
    timeout: Duration,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a [ToolSchema]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

impl LlmClient {
    pub fn new(config: &LlmConfig) -> Result<Self> {
        let timeout = Duration::from_secs(config.timeout_secs);
        let client = Client::builder().timeout(timeout).build()?;

        Ok(Self {
            client,
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            timeout,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub async fn plan(&self, messages: Vec<ChatMessage>, tools: &[ToolSchema]) -> Result<ChatMessage> {
        self.send_chat_completion(messages, Some(tools)).await
    }

    pub async fn explain(&self, messages: Vec<ChatMessage>) -> Result<String> {
        let message = self.send_chat_completion(messages, None).await?;
        message
            .content
            .filter(|content| !content.trim().is_empty())
            .ok_or_else(|| anyhow!("LLM response did not include explanation text"))
    }

    async fn send_chat_completion(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<&[ToolSchema]>,
    ) -> Result<ChatMessage> {
        let request = ChatCompletionRequest {
            model: &self.model,
            messages: &messages,
            stream: false,
            tools,
            tool_choice: tools.map(|_| "auto"),
        };

        let response = self
            .client
            .post(self.endpoint())
            .json(&request)
            .send()
            .await
            .with_context(|| format!("failed to reach Ollama at {}", self.base_url))?;

        let status = response.status();
        let body = response.text().await.context("failed to read Ollama response body")?;
        if !status.is_success() {
            return Err(anyhow!("Ollama returned {}: {}", status, body.trim()));
        }

        let parsed: ChatCompletionResponse =
            serde_json::from_str(&body).context("failed to parse Ollama response JSON")?;

        parsed
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message)
            .ok_or_else(|| anyhow!("Ollama response contained no choices"))
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}