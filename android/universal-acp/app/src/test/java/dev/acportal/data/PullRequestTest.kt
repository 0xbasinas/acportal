package dev.acportal.data

import dev.acportal.protocol.*
import org.junit.Assert.*
import org.junit.Test

class PullRequestTest {
    private fun request(owner:String="",branch:String="feature") = PullRequestRequest("/w","main",branch,owner,"Title","Body",false)

    @Test fun aForkOwnerMakesTheHeadOwnerAndBranch() {
        assertEquals("feature",request().head())
        assertEquals("octo-cat:feature",request(owner="octo-cat").head())
        assertEquals("octo-cat:feature",request(owner=" octo-cat ",branch=" feature ").head())
    }

    @Test fun theBranchIsTheThirdLineOfRevParseAndDetachedHeadIsNone() {
        assertEquals("feature/x",branchFromRevParse("/repo\nabc123\nfeature/x\n"))
        assertNull(branchFromRevParse("/repo\nabc123\nHEAD\n"))
        assertNull(branchFromRevParse("/repo\n"))
        assertNull(branchFromRevParse(""))
    }

    @Test fun pullRequestFailuresSayWhatHappenedAndNeverBlameTheController() {
        val path="/v1/pull-requests"
        val exists=hostRequestError(409,"already_exists","POST",path,existingUrl="https://github.com/o/r/pull/3")
        assertTrue(exists.contains("already exists"))
        assertTrue(exists.contains("https://github.com/o/r/pull/3"))
        assertFalse(hostRequestError(409,"already_exists","POST",path).contains("https://"))
        assertTrue(hostRequestError(409,"gh_not_signed_in","POST",path).contains("gh auth login"))
        assertTrue(hostRequestError(503,"gh_unavailable","POST",path).contains("not found"))
        assertTrue(hostRequestError(409,"no_remote","POST",path).contains("no GitHub remote"))
        assertTrue(hostRequestError(504,"gh_timeout","POST",path).contains("in time"))
        assertTrue(hostRequestError(400,"invalid_branch_name","POST",path).contains("not valid"))
        assertFalse(hostRequestError(409,"no_remote","POST",path).contains("controller"))
        assertEquals("gh could not open the pull request.",hostRequestError(502,"gh_failed","POST",path))
    }

    @Test fun theNewHostFieldsDecodeAndOlderHostsStillDo() {
        val status=WireJson.decodeFromString<HostStatus>("""{"hostId":"h","name":"n","version":"1","features":["pullRequests","booleanOptions"],"gh":"found"}""")
        assertEquals("found",status.gh)
        assertTrue(HOST_FEATURE_PULL_REQUESTS in status.features)
        assertNull(WireJson.decodeFromString<HostStatus>("""{"hostId":"h","name":"n","version":"1"}""").gh)
        val earlier=WireJson.decodeFromString<EarlierSessions>("""{"listSupported":true,"loadSupported":false,"sessions":[{"sessionId":"a","title":"First"},{"sessionId":"b","title":null}]}""")
        assertEquals(listOf("a","b"),earlier.sessions.map {it.sessionId})
        assertFalse(earlier.loadSupported)
        assertTrue(WireJson.decodeFromString<EarlierSessions>("""{}""").sessions.isEmpty())
    }
}
