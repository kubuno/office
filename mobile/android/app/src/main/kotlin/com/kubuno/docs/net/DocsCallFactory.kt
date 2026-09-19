package com.kubuno.docs.net

import com.kubuno.android.account.BrokeredClients
import com.kubuno.android.account.SharedAccounts
import javax.inject.Inject
import javax.inject.Singleton
import okhttp3.Call
import okhttp3.OkHttpClient
import okhttp3.Request

/**
 * Routes an image/byte request to the shared account that owns its URL.
 *
 * The core's [com.kubuno.android.account.AccountCallFactory] serves apps that
 * OWN their accounts: it reads the private per-app AccountRegistry. Docs is a
 * pure consumer, so that registry is empty here and the core factory would hand
 * Coil an anonymous client — every authenticated byte (document covers, inline
 * images, previews) would come back 401 with no visible cause. This factory
 * borrows the same access token the typed API calls use, so image loading works
 * at all.
 */
@Singleton
class DocsCallFactory @Inject constructor(
    private val sharedAccounts: SharedAccounts,
    private val brokered: BrokeredClients,
) : Call.Factory {

    /** For URLs belonging to no known account: no credentials attached. */
    private val anonymous by lazy { OkHttpClient() }

    override fun newCall(request: Request): Call {
        val url = request.url.toString()
        val owner = sharedAccounts.list()
            // Longest prefix wins: two instances can share a host and differ
            // only by path (`https://host/` and `https://host/kubuno`).
            .filter { url.startsWith(it.serverUrl) }
            .maxByOrNull { it.serverUrl.length }
        val client = owner?.let { brokered.of(it).okHttpClient } ?: anonymous
        return client.newCall(request)
    }
}
