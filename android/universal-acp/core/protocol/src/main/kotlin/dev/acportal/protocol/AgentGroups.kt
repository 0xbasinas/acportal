package dev.acportal.protocol

/**
 * Agents with accounts: an entry whose `profileOf` names another listed agent is shown straight
 * after that agent, in the host's order otherwise. A profile whose base is not in the list (or a
 * profile of a profile, which the host refuses) stays where it was, so nothing disappears.
 */
fun orderedByProfile(agents: List<AgentInfo>): List<AgentInfo> {
    val ids = agents.mapTo(HashSet()) { it.id }
    val under = agents.filter { it.profileOf != null && it.profileOf in ids && it.profileOf != it.id }
        .groupBy { it.profileOf }
    val nested = under.values.flatten().mapTo(HashSet()) { it.id }
    val out = ArrayList<AgentInfo>(agents.size)
    for (agent in agents) {
        if (agent.id in nested && agent.profileOf !in nested) continue
        out += agent
        under[agent.id]?.let { out += it }
    }
    // A chain (a profile of a profile) is not placed under anything above; keep it listed.
    for (agent in agents) if (agent !in out) out += agent
    return out
}

/** The name to show: "Base · Account" for a profile whose base is listed, else the agent's own name. */
fun agentTitle(agent: AgentInfo, all: List<AgentInfo>): String {
    val base = agent.profileOf?.let { id -> all.firstOrNull { it.id == id } } ?: return agent.name
    val account = agent.profileLabel?.takeIf { it.isNotBlank() } ?: agent.name
    return "${base.name} · $account"
}
