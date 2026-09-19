package com.kubuno.docs.net

import okhttp3.MultipartBody
import okhttp3.RequestBody
import okhttp3.ResponseBody
import retrofit2.http.Body
import retrofit2.http.DELETE
import retrofit2.http.GET
import retrofit2.http.Header
import retrofit2.http.Multipart
import retrofit2.http.PATCH
import retrofit2.http.POST
import retrofit2.http.Part
import retrofit2.http.Path
import retrofit2.http.Query
import retrofit2.http.Streaming

/**
 * The office module's document surface, proxied by the core at /api/v1/office.
 * Paths are relative to the client's base URL (which already ends at the host),
 * so they are never called on the module's own port.
 *
 * Image/byte streams referenced from a document are NOT modelled here: Coil
 * fetches those through the account-authenticated call factory, the same way
 * the photos app does. Only the two document EXPORTS return raw bytes, because
 * they are user-initiated downloads with no URL to hand to an image loader.
 */
interface DocsApi {

    // ── Documents ────────────────────────────────────────────────────────────

    /**
     * `limit` is capped at 200 server-side. The response's `total` counts only
     * the rows of this page, so page by comparing `documents.size` to `limit`.
     */
    @GET("api/v1/office/documents")
    suspend fun listDocuments(
        @Query("parent_id") parentId: String? = null,
        @Query("search") search: String? = null,
        @Query("starred") starred: Boolean? = null,
        @Query("trashed") trashed: Boolean? = null,
        @Query("recent") recent: Boolean? = null,
        @Query("shared") shared: Boolean? = null,
        @Query("limit") limit: Int? = null,
        @Query("offset") offset: Int? = null,
    ): DocumentListResponse

    /**
     * An `Idempotency-Key` replays the stored response instead of creating a
     * second document when a flaky connection hides the first reply.
     */
    @POST("api/v1/office/documents")
    suspend fun createDocument(
        @Body body: CreateDocumentBody,
        @Header("Idempotency-Key") idempotencyKey: String? = null,
    ): DocumentResponse

    /**
     * Incremental pull. Pass the previous response's `cursor`; `include=content`
     * inlines each document's body so a first sync needs no follow-up GETs.
     */
    @GET("api/v1/office/documents/delta")
    suspend fun delta(
        @Query("cursor") cursor: Long = 0,
        @Query("limit") limit: Int? = null,
        @Query("include") include: String? = null,
    ): DeltaResponse

    /** Opens the document backing a Drive file, importing the file on first open. */
    @POST("api/v1/office/documents/open-by-file")
    suspend fun openByFile(@Body body: OpenByFileBody): DocumentResponse

    @GET("api/v1/office/documents/{id}")
    suspend fun getDocument(@Path("id") id: String): DocumentResponse

    /**
     * `If-Match` carries the etag last seen; the server answers 412
     * PRECONDITION_FAILED when someone else wrote in between, rather than
     * silently overwriting their edit.
     */
    @PATCH("api/v1/office/documents/{id}")
    suspend fun updateDocument(
        @Path("id") id: String,
        @Body body: UpdateDocumentBody,
        @Header("If-Match") ifMatch: String? = null,
        @Header("Idempotency-Key") idempotencyKey: String? = null,
    ): DocumentResponse

    @POST("api/v1/office/documents/{id}/trash")
    suspend fun trash(@Path("id") id: String): OkResponse

    @POST("api/v1/office/documents/{id}/restore")
    suspend fun restore(@Path("id") id: String): OkResponse

    /** Permanent removal. The document must already be trashed, or this is a 404. */
    @DELETE("api/v1/office/documents/{id}/delete")
    suspend fun deleteForever(@Path("id") id: String): OkResponse

    @POST("api/v1/office/documents/{id}/duplicate")
    suspend fun duplicate(@Path("id") id: String): DocumentResponse

    // ── Import / export ──────────────────────────────────────────────────────

    /** Raw .docx bytes, streamed: the body can be large, so never buffer it whole. */
    @Streaming
    @GET("api/v1/office/documents/{id}/export/docx")
    suspend fun exportDocx(@Path("id") id: String): ResponseBody

    @Streaming
    @GET("api/v1/office/documents/{id}/export/odt")
    suspend fun exportOdt(@Path("id") id: String): ResponseBody

    /**
     * Writes the document back over the file it was imported from. Answers 422
     * VALIDATION when the origin is a `.doc` (no writer for that format) or when
     * the document was not imported at all.
     */
    @POST("api/v1/office/documents/{id}/save-source")
    suspend fun saveToSource(@Path("id") id: String): SaveSourceResponse

    /**
     * Imports one .docx/.dotx/.odt/.ott/.doc. The module reads the "file" part
     * and, optionally, a "parent_id" text part.
     */
    @Multipart
    @POST("api/v1/office/documents/import")
    suspend fun importDocument(
        @Part file: MultipartBody.Part,
        @Part("parent_id") parentId: RequestBody? = null,
    ): DocumentResponse

    // ── Versions ─────────────────────────────────────────────────────────────

    @GET("api/v1/office/documents/{id}/versions")
    suspend fun listVersions(@Path("id") id: String): VersionListResponse

    @POST("api/v1/office/documents/{id}/versions")
    suspend fun createVersion(
        @Path("id") id: String,
        @Body body: CreateVersionBody,
    ): VersionResponse

    @POST("api/v1/office/documents/{id}/versions/{versionId}/restore")
    suspend fun restoreVersion(
        @Path("id") id: String,
        @Path("versionId") versionId: String,
    ): DocumentResponse

    // ── Comments ─────────────────────────────────────────────────────────────

    @GET("api/v1/office/documents/{docId}/comments")
    suspend fun listComments(@Path("docId") docId: String): CommentListResponse

    @POST("api/v1/office/documents/{docId}/comments")
    suspend fun createComment(
        @Path("docId") docId: String,
        @Body body: CreateCommentBody,
    ): CommentResponse

    @PATCH("api/v1/office/documents/{docId}/comments/{commentId}")
    suspend fun updateComment(
        @Path("docId") docId: String,
        @Path("commentId") commentId: String,
        @Body body: UpdateCommentBody,
    ): CommentResponse

    @DELETE("api/v1/office/documents/{docId}/comments/{commentId}")
    suspend fun deleteComment(
        @Path("docId") docId: String,
        @Path("commentId") commentId: String,
    ): OkResponse

    /** Toggles the resolved flag — it is not a one-way "resolve". Owner only. */
    @POST("api/v1/office/documents/{docId}/comments/{commentId}/resolve")
    suspend fun toggleCommentResolved(
        @Path("docId") docId: String,
        @Path("commentId") commentId: String,
    ): OkResponse

    // ── Public share links ───────────────────────────────────────────────────

    @GET("api/v1/office/documents/{docId}/shares")
    suspend fun listShares(@Path("docId") docId: String): ShareListResponse

    /** 403 POLICY_DISABLED when the instance forbids public links. */
    @POST("api/v1/office/documents/{docId}/shares")
    suspend fun createShare(
        @Path("docId") docId: String,
        @Body body: CreateShareBody,
    ): ShareResponse

    @DELETE("api/v1/office/documents/{docId}/shares/{shareId}")
    suspend fun revokeShare(
        @Path("docId") docId: String,
        @Path("shareId") shareId: String,
    ): OkResponse

    // ── Collaborators ────────────────────────────────────────────────────────

    /** User search for the share sheet; an empty `q` returns an empty list. */
    @GET("api/v1/office/recipients")
    suspend fun searchRecipients(@Query("q") query: String): RecipientListResponse

    @GET("api/v1/office/documents/{docId}/collaborators")
    suspend fun listCollaborators(@Path("docId") docId: String): CollaboratorListResponse

    @POST("api/v1/office/documents/{docId}/collaborators")
    suspend fun addCollaborator(
        @Path("docId") docId: String,
        @Body body: AddCollaboratorBody,
    ): AddCollaboratorResponse

    @PATCH("api/v1/office/documents/{docId}/collaborators/{userId}")
    suspend fun updateCollaborator(
        @Path("docId") docId: String,
        @Path("userId") userId: String,
        @Body body: UpdateCollaboratorBody,
    ): OkResponse

    /** Removing yourself is allowed — that is how a collaborator leaves a share. */
    @DELETE("api/v1/office/documents/{docId}/collaborators/{userId}")
    suspend fun removeCollaborator(
        @Path("docId") docId: String,
        @Path("userId") userId: String,
    ): OkResponse

    // ── Templates ────────────────────────────────────────────────────────────

    @GET("api/v1/office/documents/templates")
    suspend fun listTemplates(): TemplateListResponse

    @POST("api/v1/office/documents/templates")
    suspend fun createTemplate(@Body body: CreateTemplateBody): TemplateResponse

    /** Only a template you created and that is not built-in can be deleted. */
    @DELETE("api/v1/office/documents/templates/{id}")
    suspend fun deleteTemplate(@Path("id") id: String): OkResponse

    // ── Editing session ──────────────────────────────────────────────────────

    /** Creates/refreshes the draft and returns the draft content plus the editors. */
    @POST("api/v1/office/documents/{id}/editing/join")
    suspend fun joinEditing(@Path("id") id: String): JoinEditingResponse

    /** Promotes the draft onto the main content file. */
    @POST("api/v1/office/documents/{id}/editing/save")
    suspend fun saveEditing(@Path("id") id: String): OkResponse

    /** Keepalive: a session goes stale after two minutes without a ping. */
    @POST("api/v1/office/documents/{id}/editing/ping")
    suspend fun pingEditing(@Path("id") id: String): OkResponse

    /** Saves the draft, drops the session, and discards the draft if last out. */
    @DELETE("api/v1/office/documents/{id}/editing/leave")
    suspend fun leaveEditing(@Path("id") id: String): OkResponse
}
