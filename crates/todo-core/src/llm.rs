use std::{collections::HashMap, env, fmt, time::Duration};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    parse, review_alignment, AlignmentVerdict, Document, Error, Evidence, Result,
    SemanticCandidate, SemanticCheck, SemanticProposal, SemanticReconciler, Verdict,
    ALIGNMENT_PROMPT,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const OPENAI_BASE_URL: &str = "https://api.openai.com";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    Anthropic,
    OpenAi,
}

impl LlmProvider {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Ok(Self::Anthropic),
            "openai" => Ok(Self::OpenAi),
            other => Err(Error::InvalidLlmConfiguration(format!(
                "unsupported provider `{other}`; expected anthropic or openai"
            ))),
        }
    }

    fn api_key_env(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAi => "OPENAI_API_KEY",
        }
    }

    fn base_url_env(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_BASE_URL",
            Self::OpenAi => "OPENAI_BASE_URL",
        }
    }

    fn model_env(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_MODEL",
            Self::OpenAi => "OPENAI_MODEL",
        }
    }

    fn default_base_url(self) -> &'static str {
        match self {
            Self::Anthropic => ANTHROPIC_BASE_URL,
            Self::OpenAi => OPENAI_BASE_URL,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
}

impl fmt::Debug for LlmConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LlmConfig")
            .field("provider", &self.provider)
            .field("api_key", &"[redacted]")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl LlmConfig {
    /// Reads BYOK configuration when `TODO_LLM_PROVIDER` is set.
    pub fn from_env() -> Result<Option<Self>> {
        let Some(provider) = env_value("TODO_LLM_PROVIDER") else {
            return Ok(None);
        };
        let provider = LlmProvider::parse(&provider)?;
        Ok(Some(Self {
            provider,
            api_key: env_value("TODO_LLM_API_KEY")
                .or_else(|| env_value(provider.api_key_env()))
                .unwrap_or_default(),
            base_url: env_value("TODO_LLM_BASE_URL")
                .or_else(|| env_value(provider.base_url_env()))
                .unwrap_or_default(),
            model: env_value("TODO_LLM_MODEL")
                .or_else(|| env_value(provider.model_env()))
                .unwrap_or_default(),
        }))
    }

    fn resolve(&self) -> std::result::Result<ResolvedConfig, String> {
        let api_key = nonempty(&self.api_key)
            .or_else(|| env_value("TODO_LLM_API_KEY"))
            .or_else(|| env_value(self.provider.api_key_env()))
            .ok_or_else(|| "authentication unavailable: API key is not configured".to_string())?;
        let model = nonempty(&self.model)
            .or_else(|| env_value("TODO_LLM_MODEL"))
            .or_else(|| env_value(self.provider.model_env()))
            .ok_or_else(|| "model is not configured".to_string())?;
        let base_url = nonempty(&self.base_url)
            .or_else(|| env_value("TODO_LLM_BASE_URL"))
            .or_else(|| env_value(self.provider.base_url_env()))
            .unwrap_or_else(|| self.provider.default_base_url().to_string());
        Ok(ResolvedConfig {
            provider: self.provider,
            api_key,
            base_url,
            model,
        })
    }
}

struct ResolvedConfig {
    provider: LlmProvider,
    api_key: String,
    base_url: String,
    model: String,
}

pub struct LlmSemanticReconciler {
    config: LlmConfig,
    client: Client,
}

impl LlmSemanticReconciler {
    pub fn new(config: LlmConfig) -> Result<Self> {
        Self::with_timeout(config, DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(config: LlmConfig, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            return Err(Error::InvalidLlmConfiguration(
                "request timeout must be greater than zero".into(),
            ));
        }
        let mut builder = Client::builder()
            .connect_timeout(timeout.min(Duration::from_secs(10)))
            .timeout(timeout);
        // A loopback endpoint is a local model (or a test double) and is
        // contacted directly. Routing it through a system proxy from the
        // developer's shell would send "localhost" to the proxy host instead,
        // and a refused connection would surface as a slow proxy error.
        if is_loopback_endpoint(&config.base_url) {
            builder = builder.no_proxy();
        }
        let client = builder.build().map_err(|_| {
            Error::InvalidLlmConfiguration("could not initialize HTTP client".into())
        })?;
        Ok(Self { config, client })
    }

    pub fn from_env() -> Result<Option<Self>> {
        LlmConfig::from_env()?.map(Self::new).transpose()
    }

    /// Whether BYOK configuration is present at all. Callers use this to tell
    /// "the semantic layer is switched off" apart from "it was configured and
    /// then silently not run", which must never be reported as a clean check.
    pub fn is_configured() -> bool {
        env_value("TODO_LLM_PROVIDER").is_some()
    }

    fn request(&self, candidates: &[SemanticCandidate]) -> std::result::Result<String, String> {
        let config = self.config.resolve()?;
        let endpoint = provider_endpoint(&config.base_url, config.provider);
        let candidate_message = serde_json::to_string_pretty(&json!({
            "schema": "todo.semantic-candidates.v1",
            "instruction": "Return todo.alignment.v1 JSON for every candidate below.",
            "candidates": candidates,
        }))
        .map_err(|_| "candidate payload could not be encoded".to_string())?;

        let request = match config.provider {
            LlmProvider::Anthropic => self
                .client
                .post(endpoint)
                .header("x-api-key", &config.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&json!({
                    "model": config.model,
                    "max_tokens": 4096,
                    "system": ALIGNMENT_PROMPT,
                    "messages": [{"role": "user", "content": candidate_message}],
                })),
            LlmProvider::OpenAi => self
                .client
                .post(endpoint)
                .bearer_auth(&config.api_key)
                .json(&json!({
                    "model": config.model,
                    "messages": [
                        {"role": "system", "content": ALIGNMENT_PROMPT},
                        {"role": "user", "content": candidate_message}
                    ],
                })),
        };
        let response = request.send().map_err(classify_transport_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(classify_http_status(status.as_u16()));
        }
        let envelope: Value = response
            .json()
            .map_err(|_| "provider response was not valid JSON".to_string())?;
        extract_assistant_text(config.provider, &envelope)
    }
}

impl SemanticReconciler for LlmSemanticReconciler {
    fn reconcile(&self, document: &Document, candidates: &[SemanticCandidate]) -> SemanticCheck {
        let candidates = unique_candidates(candidates);
        if candidates.is_empty() {
            return SemanticCheck::completed(Vec::new());
        }
        if candidates
            .iter()
            .any(|candidate| document.task(&candidate.task_id).is_none())
        {
            return SemanticCheck::skipped("candidate set no longer matches the document");
        }
        let response = match self.request(&candidates) {
            Ok(response) => response,
            Err(reason) => return SemanticCheck::skipped(reason),
        };
        let candidate_document = match candidate_document(&candidates) {
            Ok(document) => document,
            Err(_) => return SemanticCheck::skipped("candidate validation could not be prepared"),
        };
        let review = match review_alignment(&candidate_document, &response) {
            Ok(review) => review,
            Err(_) => return SemanticCheck::skipped("model response failed alignment validation"),
        };

        let proposals = review
            .accepted
            .into_iter()
            .filter_map(proposal_from_verdict)
            .collect();
        let mut checks_skipped = Vec::new();
        if !review.rejected.is_empty() || !review.missing.is_empty() || review.truncated {
            checks_skipped.push(format!(
                "model response was incomplete: {} rejected, {} missing, truncated={}",
                review.rejected.len(),
                review.missing.len(),
                review.truncated
            ));
        }
        SemanticCheck {
            proposals,
            checks_skipped,
        }
    }
}

fn unique_candidates(candidates: &[SemanticCandidate]) -> Vec<SemanticCandidate> {
    let mut unique: Vec<SemanticCandidate> = Vec::new();
    let mut indices: HashMap<String, usize> = HashMap::new();
    for candidate in candidates {
        if let Some(index) = indices.get(&candidate.task_id).copied() {
            unique[index].evidence.extend(candidate.evidence.clone());
        } else {
            indices.insert(candidate.task_id.clone(), unique.len());
            unique.push(candidate.clone());
        }
    }
    unique
}

fn candidate_document(candidates: &[SemanticCandidate]) -> Result<Document> {
    let mut source = String::from("---\ntodo-spec: 1\ncolumns: []\n---\n");
    for candidate in candidates {
        source.push_str(&format!(
            "- [ ] semantic candidate <!--t:{}-->\n",
            candidate.task_id
        ));
    }
    parse(source)
}

fn proposal_from_verdict(verdict: AlignmentVerdict) -> Option<SemanticProposal> {
    if matches!(verdict.verdict, Verdict::Unknown | Verdict::NotStarted) {
        return None;
    }
    let judgment = match verdict.verdict {
        Verdict::Done => "semantic-done",
        Verdict::Partial => "semantic-partial",
        Verdict::EvidenceBroken => "semantic-evidence-broken",
        Verdict::Stale => "semantic-stale",
        Verdict::Unknown | Verdict::NotStarted => unreachable!(),
    };
    let mut evidence = vec![Evidence {
        kind: "semantic".into(),
        value: verdict.reason,
        path: None,
        line: None,
    }];
    evidence.extend(verdict.citations.into_iter().filter_map(|citation| {
        let (path, line) = citation.rsplit_once(':')?;
        Some(Evidence {
            kind: "citation".into(),
            value: citation.clone(),
            path: Some(path.to_string()),
            line: line.parse().ok(),
        })
    }));
    Some(SemanticProposal {
        task_id: verdict.task_id,
        judgment: judgment.into(),
        evidence,
        completed: (verdict.verdict == Verdict::Done).then_some(true),
    })
}

fn is_loopback_endpoint(base_url: &str) -> bool {
    let without_scheme = base_url
        .split_once("://")
        .map_or(base_url, |(_, rest)| rest);
    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']').map(|(h, _)| h))
        .unwrap_or_else(|| host.rsplit_once(':').map_or(host, |(h, _)| h));
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn provider_endpoint(base_url: &str, provider: LlmProvider) -> String {
    let base = base_url.trim_end_matches('/');
    let endpoint = match provider {
        LlmProvider::Anthropic => "messages",
        LlmProvider::OpenAi => "chat/completions",
    };
    if base.ends_with(endpoint) {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/{endpoint}")
    } else {
        format!("{base}/v1/{endpoint}")
    }
}

fn extract_assistant_text(
    provider: LlmProvider,
    envelope: &Value,
) -> std::result::Result<String, String> {
    let text = match provider {
        LlmProvider::Anthropic => envelope["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|part| part["type"] == "text")
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        LlmProvider::OpenAi => {
            let content = &envelope["choices"][0]["message"]["content"];
            content.as_str().map(str::to_string).unwrap_or_else(|| {
                content
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
    };
    if text.trim().is_empty() {
        Err("provider response contained no assistant text".into())
    } else {
        Ok(text)
    }
}

fn classify_transport_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "provider request timed out".into()
    } else if error.is_connect() {
        "provider network connection failed".into()
    } else {
        "provider network request failed".into()
    }
}

fn classify_http_status(status: u16) -> String {
    match status {
        401 | 403 => format!("authentication failed (HTTP {status})"),
        408 => "provider request timed out (HTTP 408)".into(),
        429 => "provider rate limited the semantic check (HTTP 429)".into(),
        500..=599 => format!("provider unavailable (HTTP {status})"),
        _ => format!("provider rejected the semantic check (HTTP {status})"),
    }
}

fn env_value(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|value| nonempty(&value))
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}
