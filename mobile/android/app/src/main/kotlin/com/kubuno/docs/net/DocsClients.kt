package com.kubuno.docs.net

import com.kubuno.android.account.BrokeredClient
import com.kubuno.android.account.BrokeredClients
import com.kubuno.android.account.SharedAccount
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import retrofit2.Retrofit
import retrofit2.converter.kotlinx.serialization.asConverterFactory

/**
 * Builds a [DocsApi] per shared account, over that account's borrowed-token
 * client. Docs is a consumer app: it owns no account and holds no refresh
 * token, so every call rides a short-lived access token brokered by
 * :core-account from the app that actually signed the account in.
 */
@Singleton
class DocsClients @Inject constructor(
    private val brokered: BrokeredClients,
) {
    /**
     * Unknown keys are ignored and nulls are dropped on the way out: the office
     * module keeps gaining document fields, and a client that has not been
     * rebuilt must keep parsing responses instead of failing the whole screen.
     * Omitting nulls also matters for PATCH, whose DTO is a partial update —
     * an explicit null there would clear a field the user never touched.
     */
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
    }

    private val apis = ConcurrentHashMap<String, DocsApi>()

    fun api(account: SharedAccount): DocsApi = apis.computeIfAbsent(account.systemName) {
        val client = brokered.of(account)
        Retrofit.Builder()
            // BrokeredClient.serverUrl has no trailing slash, and DocsApi paths
            // are relative ("api/v1/office/..."), so Retrofit needs the slash.
            .baseUrl(client.serverUrl + "/")
            .client(client.okHttpClient)
            .addConverterFactory(json.asConverterFactory("application/json".toMediaType()))
            .build()
            .create(DocsApi::class.java)
    }

    /**
     * The authenticated client itself, for raw byte streams (docx/odt exports,
     * cover images) that bypass the typed API.
     */
    fun raw(account: SharedAccount): BrokeredClient = brokered.of(account)

    /**
     * Drops the cached API and its underlying client for [account].
     *
     * Needed after a re-authentication or an account switch: the Retrofit
     * instance captured the old OkHttp client, whose bearer still serves the
     * dead session, so it would keep 401-ing until the process restarts.
     */
    fun evict(account: SharedAccount) {
        apis.remove(account.systemName)
        brokered.reset(account)
    }

    /** Drops every cached API, e.g. when the account list itself changed. */
    fun evictAll() {
        apis.clear()
    }
}
