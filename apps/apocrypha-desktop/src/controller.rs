//! Application state and the flows the window can ask for.
//!
//! Ported from the Android client's `AppController.java` so that phone and
//! desktop agree on when a message is unconfirmed, when history is
//! authoritative, and what the person is told. Every method returns a full
//! [`View`]; the window renders that and holds no state of its own, which is
//! what keeps the access token out of the webview entirely.

use serde::Serialize;

use crate::api::{ApiClient, ApiError};
use crate::journal::RequestJournal;
use crate::protocol::{self, Conversation, Message};
use crate::session::AuthSession;
use crate::store::SecureStore;

const MAX_KNOWN: usize = 100;
const TITLE_CHARS: usize = 72;

#[derive(Debug, Clone, Serialize, Default)]
pub struct View {
    pub configured: bool,
    pub signed_in: bool,
    pub email: String,
    pub email_draft: String,
    pub code_sent: bool,
    pub access_denied: bool,
    pub notice: String,
    pub session_id: String,
    pub pending_request: String,
    pub history_scope: String,
    pub messages: Vec<Message>,
    pub conversations: Vec<Conversation>,
}

pub struct Controller {
    store: SecureStore,
    api: ApiClient,
    journal: RequestJournal,
    session: Option<AuthSession>,
    known: Vec<(String, String)>,
    messages: Vec<Message>,
    notice: String,
    email_draft: String,
    session_id: String,
    history_scope: String,
    fresh_conversation: bool,
    code_sent: bool,
    access_denied: bool,
}

type Flow = std::result::Result<(), ApiError>;

impl Controller {
    pub fn new() -> std::result::Result<Self, String> {
        Ok(Self {
            store: SecureStore::new().map_err(|error| error.to_string())?,
            api: ApiClient::new(),
            journal: RequestJournal::default(),
            session: None,
            known: Vec::new(),
            messages: Vec::new(),
            notice: String::new(),
            email_draft: String::new(),
            session_id: protocol::new_id(),
            history_scope: "account_conversations".to_string(),
            fresh_conversation: false,
            code_sent: false,
            access_denied: false,
        })
    }

    pub fn view(&self) -> View {
        let pending = self.journal.request_for(&self.session_id);
        let mut conversations: Vec<Conversation> = self
            .known
            .iter()
            .map(|(id, title)| Conversation { id: id.clone(), title: title.clone() })
            .collect();
        conversations.reverse();
        View {
            configured: self.api.configured(),
            signed_in: self.session.is_some(),
            email: self.session.as_ref().map(|s| s.email.clone()).unwrap_or_default(),
            email_draft: self.email_draft.clone(),
            code_sent: self.code_sent,
            access_denied: self.access_denied,
            notice: self.notice.clone(),
            session_id: self.session_id.clone(),
            pending_request: pending,
            history_scope: self.history_scope.clone(),
            messages: self.messages.clone(),
            conversations,
        }
    }

    /// Runs a flow and turns any failure into the notice the person sees.
    fn finish(&mut self, outcome: Flow) -> View {
        if let Err(error) = outcome {
            self.notice = error.to_string();
            if let ApiError::Http { status: 403, .. } = error {
                self.access_denied = true;
            }
            if !self.journal.request_for(&self.session_id).is_empty() {
                self.notice.push_str(
                    " Your message is unconfirmed. Check the reply; it will not be sent again automatically.",
                );
            }
        }
        self.view()
    }

    pub fn initialize(&mut self) -> View {
        let outcome = (|| -> Flow {
            self.api.configure()?;
            let saved = match self.store.get("auth")? {
                Some(value) => value,
                None => {
                    self.notice = "Sign in with your apocky.com account.".into();
                    return Ok(());
                }
            };
            let refresh = protocol::text(&saved, "refresh_token", 16_384)?;
            let user = protocol::text(&saved, "user_id", 64)?;
            self.session = Some(self.api.refresh(&refresh, &user)?);
            self.persist_session()?;
            self.load_refs()?;
            self.refresh_account()
        })();
        self.finish(outcome)
    }

    pub fn send_code(&mut self, address: &str, create: bool) -> View {
        let outcome = (|| -> Flow {
            self.email_draft = protocol::email(address)?;
            if create {
                self.api.create_account(&self.email_draft)?;
                self.code_sent = true;
                self.notice = "Check your email to verify your account. Enter the numeric code here if one is included.".into();
            } else {
                self.api.send_code(&self.email_draft)?;
                self.code_sent = true;
                self.notice = "Check your email. Enter the numeric code if one is included. If you received only a link, use your password below.".into();
            }
            Ok(())
        })();
        self.finish(outcome)
    }

    pub fn create_with_password(&mut self, address: &str, password: &str) -> View {
        let outcome = (|| -> Flow {
            self.email_draft = protocol::email(address)?;
            match self.api.create_with_password(&self.email_draft, password)? {
                None => {
                    self.code_sent = true;
                    self.notice = "Check your email and confirm your account. Then return here and sign in with your password.".into();
                    Ok(())
                }
                Some(incoming) => self.adopt(incoming),
            }
        })();
        self.finish(outcome)
    }

    pub fn sign_in(&mut self, address: &str, secret: &str, password: bool) -> View {
        let outcome = (|| -> Flow {
            self.email_draft = protocol::email(address)?;
            let incoming = if password {
                self.api.password(&self.email_draft, secret)?
            } else {
                self.api.verify_code(&self.email_draft, secret)?
            };
            self.adopt(incoming)
        })();
        self.finish(outcome)
    }

    /// Takes on a newly verified session, discarding another account's state.
    fn adopt(&mut self, incoming: AuthSession) -> Flow {
        let switched = self
            .session
            .as_ref()
            .map(|current| current.user_id != incoming.user_id)
            .unwrap_or(true);
        if switched {
            self.known.clear();
            self.messages.clear();
            self.journal.clear();
        }
        self.session = Some(incoming);
        self.persist_session()?;
        self.load_refs()?;
        self.refresh_account()
    }

    pub fn refresh(&mut self) -> View {
        let outcome = self.refresh_account();
        self.finish(outcome)
    }

    pub fn open_conversation(&mut self, id: &str) -> View {
        let outcome = (|| -> Flow {
            self.session_id = protocol::id(id)?;
            self.fresh_conversation = false;
            self.messages.clear();
            self.fresh_session()?;
            self.read_conversation()?;
            self.save_refs()
        })();
        self.finish(outcome)
    }

    pub fn new_conversation(&mut self) -> View {
        self.session_id = protocol::new_id();
        self.fresh_conversation = true;
        self.messages.clear();
        self.access_denied = false;
        self.notice = "A fresh conversation. Any unconfirmed replies remain available in history.".into();
        self.view()
    }

    pub fn send(&mut self, value: &str) -> View {
        if self.session.is_none() || self.access_denied || !self.journal.request_for(&self.session_id).is_empty() {
            return self.view();
        }
        let outcome = self.send_turn(value);
        self.finish(outcome)
    }

    fn send_turn(&mut self, value: &str) -> Flow {
        let text = protocol::prompt(value)?;
        let request = protocol::new_id();
        let target = self.session_id.clone();
        self.fresh_session()?;
        self.journal.begin(&target, &request)?;
        self.fresh_conversation = false;
        let title: String = text.chars().take(TITLE_CHARS).collect();
        self.remember(&target, &title);
        self.save_refs()?;
        self.messages.push(Message {
            role: "user".into(),
            content: text.clone(),
            request_id: request.clone(),
        });

        let token = self.access_token()?;
        let body = protocol::turn_body(&text, &target, &request)?;
        let result = match self.api.account("/turn", Some(body), &token) {
            Ok(result) => result,
            Err(error) => {
                // These statuses mean the service never accepted the message,
                // so it is safe to release it and hand the text back. Any other
                // failure leaves the turn unconfirmed on purpose.
                if let ApiError::Http { status, .. } = error {
                    if matches!(status, 400 | 401 | 403 | 404 | 415 | 429) {
                        self.journal.resolve(&target, &request);
                        self.messages.retain(|item| item.request_id != request);
                        self.save_refs()?;
                    }
                }
                return Err(error);
            }
        };
        let reply = protocol::completed(&result, &target, &request)?;
        self.messages.push(Message {
            role: "assistant".into(),
            content: reply,
            request_id: request.clone(),
        });
        self.journal.resolve(&target, &request);
        self.save_refs()?;
        self.notice = "Reply received.".into();
        Ok(())
    }

    pub fn sign_out(&mut self) -> View {
        let token = self.session.as_ref().map(|s| s.access_token.clone());
        self.session = None;
        self.messages.clear();
        self.known.clear();
        self.journal.clear();
        self.email_draft.clear();
        self.code_sent = false;
        self.access_denied = false;
        self.session_id = protocol::new_id();
        let mut result =
            "Signed out. Saved credentials and conversation references were removed from this computer.".to_string();
        if let Err(error) = self.store.clear() {
            result = error.to_string();
        }
        if let Some(token) = token {
            if self.api.logout(&token).is_err() {
                result.push_str(" Server sign-out could not be confirmed.");
            }
        }
        self.notice = result;
        self.view()
    }

    fn refresh_account(&mut self) -> Flow {
        self.fresh_session()?;
        let token = self.access_token()?;
        let status = self.api.account("/status", None, &token)?;
        if protocol::text(&status, "schema_version", 64)? != "apocky.mobile.status.v1" {
            return Err(ApiError::Rejected("The service returned an unsupported response.".into()));
        }
        self.access_denied = false;
        if protocol::text(&status, "status", 32)? != "live" {
            self.notice = "Apocrypha is reconnecting. Your account is signed in; retry when the service is ready.".into();
            return Ok(());
        }
        let list = self.api.account("/sessions", None, &token)?;
        let remote = protocol::sessions(&list)?;
        self.history_scope = remote.scope;
        for item in remote.conversations.iter().rev() {
            self.remember(&item.id, &item.title);
        }
        let pending = self.journal.request_for(&self.session_id);
        let unknown = !self.known.iter().any(|(id, _)| id == &self.session_id);
        if pending.is_empty() && !self.fresh_conversation && unknown && self.messages.is_empty() {
            if let Some(first) = remote.conversations.first() {
                self.session_id = first.id.clone();
            }
        }
        let selected_known = self.known.iter().any(|(id, _)| id == &self.session_id);
        if selected_known || !self.journal.request_for(&self.session_id).is_empty() {
            self.read_conversation()?;
        } else {
            self.notice = "Connected. Start a conversation with Apocrypha.".into();
        }
        self.save_refs()
    }

    fn read_conversation(&mut self) -> Flow {
        let token = self.access_token()?;
        let path = format!("/sessions?session_id={}", protocol::id(&self.session_id)?);
        let response = self.api.account(&path, None, &token)?;
        let canonical = protocol::history(&response, &self.session_id)?;
        self.messages = canonical.messages;
        let pending = self.journal.request_for(&self.session_id);
        if !pending.is_empty() {
            let answered = self
                .messages
                .iter()
                .any(|item| item.role == "assistant" && item.request_id == pending);
            if answered {
                let session_id = self.session_id.clone();
                self.journal.resolve(&session_id, &pending);
            }
        }
        self.notice = if !self.journal.request_for(&self.session_id).is_empty() {
            "The reply is not confirmed yet. Check again later; the app will not resend your message.".into()
        } else if canonical.truncated {
            "Recent messages loaded. Earlier history was truncated by the service.".into()
        } else {
            "Conversation is up to date.".into()
        };
        Ok(())
    }

    fn access_token(&self) -> std::result::Result<String, ApiError> {
        self.session
            .as_ref()
            .map(|session| session.access_token.clone())
            .ok_or_else(|| ApiError::Rejected("Sign in to continue.".into()))
    }

    /// Renews the access token when it is within a minute of expiry.
    fn fresh_session(&mut self) -> Flow {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs() as i64)
            .unwrap_or_default();
        let current = self
            .session
            .as_ref()
            .ok_or_else(|| ApiError::Rejected("Sign in to continue.".into()))?;
        if current.expires_at > now + 60 {
            return Ok(());
        }
        let renewed = self.api.refresh(&current.refresh_token, &current.user_id)?;
        self.session = Some(renewed);
        self.persist_session()
    }

    fn persist_session(&self) -> Flow {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| ApiError::Rejected("Sign in to continue.".into()))?;
        self.store.put("auth", &session.saved()).map_err(ApiError::from)
    }

    fn remember(&mut self, id: &str, title: &str) {
        self.known.retain(|(known, _)| known != id);
        self.known.push((id.to_string(), title.to_string()));
        while self.known.len() > MAX_KNOWN {
            let removable = self
                .known
                .iter()
                .position(|(id, _)| self.journal.request_for(id).is_empty());
            match removable {
                Some(index) => {
                    self.known.remove(index);
                }
                None => break,
            }
        }
    }

    fn refs_name(&self) -> std::result::Result<String, ApiError> {
        self.session
            .as_ref()
            .map(|session| format!("refs:{}", session.user_id))
            .ok_or_else(|| ApiError::Rejected("Sign in to continue.".into()))
    }

    fn load_refs(&mut self) -> Flow {
        self.known.clear();
        self.journal.clear();
        self.fresh_conversation = false;
        let name = self.refs_name()?;
        let refs = match self.store.get(&name)? {
            Some(value) => value,
            None => return Ok(()),
        };
        if let Some(ids) = refs.get("ids").and_then(|value| value.as_array()) {
            for id in ids.iter().take(MAX_KNOWN) {
                let id = protocol::id(id.as_str().unwrap_or_default())?;
                self.remember(&id, "Conversation saved on this computer");
            }
        }
        if let Some(pending) = refs.get("pending_requests") {
            self.journal.restore(pending)?;
        }
        if let Some(selected) = refs.get("selected").and_then(|value| value.as_str()) {
            if !selected.is_empty() {
                self.session_id = protocol::id(selected)?;
            }
        }
        Ok(())
    }

    fn save_refs(&self) -> Flow {
        let name = self.refs_name()?;
        let ids: Vec<&String> = self.known.iter().map(|(id, _)| id).collect();
        let payload = serde_json::json!({
            "ids": ids,
            "selected": self.session_id,
            "pending_requests": self.journal.json(),
        });
        self.store.put(&name, &payload).map_err(ApiError::from)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_signed_out_client_refuses_to_send() {
        let mut controller = Controller::new().unwrap();
        let before = controller.view().messages.len();
        let view = controller.send("hello");
        assert_eq!(view.messages.len(), before, "no message may be composed without a session");
        assert!(!view.signed_in);
    }

    #[test]
    fn a_new_conversation_takes_a_fresh_identifier() {
        let mut controller = Controller::new().unwrap();
        let first = controller.view().session_id;
        let view = controller.new_conversation();
        assert_ne!(view.session_id, first);
        assert!(protocol::id(&view.session_id).is_ok());
        assert!(view.messages.is_empty());
    }

    #[test]
    fn known_conversations_stay_newest_first_and_bounded() {
        let mut controller = Controller::new().unwrap();
        let ids: Vec<String> = (0..3).map(|_| protocol::new_id()).collect();
        for (index, id) in ids.iter().enumerate() {
            controller.remember(id, &format!("title {index}"));
        }
        let listed = controller.view().conversations;
        assert_eq!(listed[0].id, ids[2], "the most recent conversation leads the list");
        assert_eq!(listed[2].id, ids[0]);
        for _ in 0..MAX_KNOWN * 2 {
            controller.remember(&protocol::new_id(), "filler");
        }
        assert_eq!(controller.view().conversations.len(), MAX_KNOWN);
    }

    #[test]
    fn re_remembering_a_conversation_moves_it_to_the_front_without_duplicating() {
        let mut controller = Controller::new().unwrap();
        let first = protocol::new_id();
        let second = protocol::new_id();
        controller.remember(&first, "first");
        controller.remember(&second, "second");
        controller.remember(&first, "first again");
        let listed = controller.view().conversations;
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, first);
        assert_eq!(listed[0].title, "first again");
    }
}
