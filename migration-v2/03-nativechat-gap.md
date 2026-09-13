# NativeChat today vs OpenGrok

## What NativeChat owns now

**Sqlite (app support dir)**

| Table | Meaning |
|-------|---------|
| `credentials` | Provider API keys (openai / anthropic / gemini) |
| `profiles` | Named bundle: credential ids + text/embed/image/tts model ids + tts voice |
| `settings` | kv |
| `chat_sessions` | UUID + title + timestamps. **No profile/agent id** |
| `chat_messages` | session_id, role, content, optional model/provider strings |
| `models` | Local seeder of vendor model ids |

**Turn path:** `active_profile` → `text_credential.api_key` → `GeminiProvider` (OpenAI/Anthropic still stubs). Env keys as fallback. Model registry hits vendor APIs with those keys.

**UI:** sidebar = session list; header = profile Select; Account Settings = local person; Credentials modal = keys; Profile Settings = LLM bundle + TTS.

**Keep as shell:** GPUI window, virtualized transcript, composer, theme, TTS as *presentation*, gpui-agent (off by default).

## Drop from the product (not wrap)

- `credentials` as the way inference happens
- `profiles` as “which key and model”
- `LlmProvider` / Gemini client on the chat path
- Model seeder against Gemini/OpenAI
- Env `*_API_KEY` for chat
- Treating `chat_sessions` as the only list in the rail

## Rebind

| Surface | Becomes |
|---------|---------|
| Account Settings | `/account` + password + sign out |
| Sign in | `/auth/login` |
| Profile Settings | Hire / edit coworker (name, role, pin, template) |
| Sidebar list | `GET /coworkers` |
| Composer send | `POST /ag-ui` for the selected coworker |
| Theme | Keep; default dark to match Open Grok |

## Look/feel to copy (interaction, not CSS)

From the live Grok window: dark shell, **coworker avatar rail**, header = the being’s name, composer “Message {name}”, later a right pane for computer + routines. NativeChat’s “Default” profile dropdown is the wrong grammar.

## TTS

Stays local for now (native macOS + optional Gemini TTS). Do not block login/inference on moving TTS to the server.
