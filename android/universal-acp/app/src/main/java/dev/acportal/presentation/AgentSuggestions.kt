package dev.acportal.presentation

/**
 * A starting point for the Add agent form. Choosing one only fills in the fields: nothing is
 * installed, downloaded or registered, and the user still checks and saves. The list follows the
 * public ACP agent registry (agentclientprotocol.com/get-started/registry), limited to agents whose
 * registry entry gives one clear command for ACP mode once the CLI is installed. It is deliberately
 * not a list of defaults.
 */
internal data class AgentSuggestion(
    val name:String,
    val command:String,
    val args:List<String> = emptyList(),
    val env:Map<String,String> = emptyMap(),
    /** The agent edits files and runs commands with its own tools, outside this app's approvals. */
    val ownTools:Boolean=false,
)

internal val agentSuggestions=listOf(
    AgentSuggestion("Goose","goose",listOf("acp"),mapOf("GOOSE_MODE" to "approve")),
    AgentSuggestion("Gemini CLI","gemini",listOf("--acp")),
    AgentSuggestion("OpenCode","opencode",listOf("acp"),ownTools=true),
    AgentSuggestion("Kimi CLI","kimi",listOf("acp")),
    AgentSuggestion("Cursor","cursor-agent",listOf("acp")),
    AgentSuggestion("Kilo","kilo",listOf("acp")),
    AgentSuggestion("Cline","cline",listOf("--acp")),
    AgentSuggestion("Copilot","copilot",listOf("--acp")),
    AgentSuggestion("Auggie CLI","auggie",listOf("--acp")),
    AgentSuggestion("Qwen Code","qwen",listOf("--acp")),
    AgentSuggestion("Junie","junie",listOf("--acp=true")),
    AgentSuggestion("Mistral Vibe","vibe-acp"),
)
