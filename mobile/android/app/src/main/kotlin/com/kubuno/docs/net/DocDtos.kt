package com.kubuno.docs.net

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

// Wire models for the office module's document surface, proxied by the core at
// /api/v1/office. Field names mirror the Rust structs in office/src/models and
// the inline DTOs declared in office/src/handlers.
//
// Every optional field carries a default so a server that stops sending a key
// (or a response shape that predates it) still deserializes instead of throwing
// on a whole screen's worth of data.

// ─────────────────────────────────────────────────────────────────────────────
// Documents
// ─────────────────────────────────────────────────────────────────────────────

/**
 * A document row. `etag` and `content_etag` are NOT columns: the module injects
 * them into the serialized object on the responses that can be written back
 * (create/update/delta), and omits them elsewhere — hence nullable.
 */
@Serializable
data class DocumentDto(
    val id: String,
    @SerialName("owner_id") val ownerId: String = "",
    val title: String = "",
    val icon: String? = null,
    @SerialName("cover_url") val coverUrl: String? = null,
    @SerialName("word_count") val wordCount: Int = 0,
    @SerialName("is_starred") val isStarred: Boolean = false,
    @SerialName("is_trashed") val isTrashed: Boolean = false,
    @SerialName("trashed_at") val trashedAt: String? = null,
    @SerialName("parent_id") val parentId: String? = null,
    val position: Double = 0.0,
    @SerialName("last_editor_id") val lastEditorId: String? = null,
    @SerialName("file_id") val fileId: String? = null,
    @SerialName("draft_file_id") val draftFileId: String? = null,
    /** Format the document was imported from ("docx"/"odt"/"doc"); null when born here. */
    @SerialName("source_format") val sourceFormat: String? = null,
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("updated_at") val updatedAt: String? = null,
    val etag: String? = null,
    @SerialName("content_etag") val contentEtag: String? = null,
) {
    /** True when the document came from an imported file that we can rewrite in place. */
    val canSaveToSource: Boolean
        get() = sourceFormat == "docx" || sourceFormat == "odt"
}

/**
 * The lighter row returned by the listing. It carries `source_file_id` (the file
 * the user imported), which the full [DocumentDto] does not expose.
 */
@Serializable
data class DocumentSummaryDto(
    val id: String,
    @SerialName("owner_id") val ownerId: String = "",
    val title: String = "",
    val icon: String? = null,
    @SerialName("word_count") val wordCount: Int = 0,
    @SerialName("file_id") val fileId: String? = null,
    @SerialName("source_file_id") val sourceFileId: String? = null,
    @SerialName("is_starred") val isStarred: Boolean = false,
    @SerialName("is_trashed") val isTrashed: Boolean = false,
    @SerialName("parent_id") val parentId: String? = null,
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("updated_at") val updatedAt: String? = null,
)

/**
 * `GET /documents` → `{ documents, total }`.
 *
 * `total` is `rows.len()` of THIS page, not a grand total: never derive a page
 * count from it. Use `documents.size == limit` to decide whether to ask for more.
 */
@Serializable
data class DocumentListResponse(
    val documents: List<DocumentSummaryDto> = emptyList(),
    val total: Int = 0,
)

/**
 * The single-document envelope shared by get/create/update/duplicate/import/
 * open-by-file/restore-version.
 *
 * `content_json` is raw ProseMirror JSON, possibly wrapped in the editor's
 * multi-page envelope (`{_type:"multi-page", pages:[{content:<doc>}], …}`).
 * It stays a [JsonElement] on purpose: the document model is parsed by a
 * dedicated layer, not by this wire type.
 */
@Serializable
data class DocumentResponse(
    val document: DocumentDto,
    @SerialName("content_json") val contentJson: JsonElement? = null,
)

/** Body for `POST /documents`. */
@Serializable
data class CreateDocumentBody(
    val title: String? = null,
    val icon: String? = null,
    @SerialName("parent_id") val parentId: String? = null,
    @SerialName("template_id") val templateId: String? = null,
)

/**
 * Body for `PATCH /documents/:id`. Absent fields are left untouched by the
 * server (COALESCE), so only send what actually changed.
 */
@Serializable
data class UpdateDocumentBody(
    val title: String? = null,
    val icon: String? = null,
    @SerialName("cover_url") val coverUrl: String? = null,
    @SerialName("content_json") val contentJson: JsonElement? = null,
    @SerialName("parent_id") val parentId: String? = null,
    @SerialName("is_starred") val isStarred: Boolean? = null,
)

/** Body for `POST /documents/open-by-file` — opens (or imports) a Drive file. */
@Serializable
data class OpenByFileBody(
    @SerialName("file_id") val fileId: String,
)

/** The `{ "ok": true }` acknowledgement returned by trash/restore/delete/ping/… */
@Serializable
data class OkResponse(
    val ok: Boolean = false,
    /** Present on `editing/save` when there was no draft to promote. */
    val note: String? = null,
)

/** `POST /documents/:id/save-source` → `{ saved, format }`. */
@Serializable
data class SaveSourceResponse(
    val saved: Boolean = false,
    /** The format actually written back: "docx" or "odt" (never "doc"). */
    val format: String? = null,
)

// ─────────────────────────────────────────────────────────────────────────────
// Delta (incremental pull)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * One entry of the delta stream. `kind` is "modified" | "trashed" | "deleted";
 * a "deleted" entry is a tombstone and carries only `uuid` and `change_seq`,
 * which is why every other field is optional here.
 */
@Serializable
data class DeltaChangeDto(
    val uuid: String,
    val kind: String = DocChangeKind.MODIFIED,
    val etag: String? = null,
    @SerialName("content_etag") val contentEtag: String? = null,
    @SerialName("change_seq") val changeSeq: Long = 0,
    val document: DocumentDto? = null,
    /** Only present when the request asked for `include=content`. */
    @SerialName("content_json") val contentJson: JsonElement? = null,
)

/** `GET /documents/delta` → `{ changes, cursor, has_more }`. */
@Serializable
data class DeltaResponse(
    val changes: List<DeltaChangeDto> = emptyList(),
    val cursor: Long = 0,
    @SerialName("has_more") val hasMore: Boolean = false,
)

/** Values of [DeltaChangeDto.kind]. */
object DocChangeKind {
    const val MODIFIED = "modified"
    const val TRASHED = "trashed"
    const val DELETED = "deleted"
}

// ─────────────────────────────────────────────────────────────────────────────
// Versions
// ─────────────────────────────────────────────────────────────────────────────

@Serializable
data class DocumentVersionDto(
    val id: String,
    @SerialName("document_id") val documentId: String = "",
    @SerialName("author_id") val authorId: String = "",
    @SerialName("content_json") val contentJson: JsonElement? = null,
    @SerialName("word_count") val wordCount: Int = 0,
    val label: String? = null,
    @SerialName("created_at") val createdAt: String? = null,
)

/** `GET /documents/:id/versions` → `{ versions }` (50 most recent, newest first). */
@Serializable
data class VersionListResponse(
    val versions: List<DocumentVersionDto> = emptyList(),
)

/** `POST /documents/:id/versions` → `{ version }`. */
@Serializable
data class VersionResponse(
    val version: DocumentVersionDto,
)

/** Body for `POST /documents/:id/versions`. */
@Serializable
data class CreateVersionBody(
    val label: String? = null,
)

// ─────────────────────────────────────────────────────────────────────────────
// Comments
// ─────────────────────────────────────────────────────────────────────────────

@Serializable
data class CommentDto(
    val id: String,
    @SerialName("document_id") val documentId: String = "",
    @SerialName("author_id") val authorId: String = "",
    /** Set on a reply; null on a thread root. */
    @SerialName("parent_id") val parentId: String? = null,
    val content: String = "",
    @SerialName("is_resolved") val isResolved: Boolean = false,
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("updated_at") val updatedAt: String? = null,
)

@Serializable
data class CommentListResponse(
    val comments: List<CommentDto> = emptyList(),
)

@Serializable
data class CommentResponse(
    val comment: CommentDto,
)

/** Body for `POST /documents/:doc_id/comments`. */
@Serializable
data class CreateCommentBody(
    val content: String,
    @SerialName("parent_id") val parentId: String? = null,
)

/** Body for `PATCH /documents/:doc_id/comments/:id`. */
@Serializable
data class UpdateCommentBody(
    val content: String,
)

// ─────────────────────────────────────────────────────────────────────────────
// Public share links
// ─────────────────────────────────────────────────────────────────────────────

@Serializable
data class ShareDto(
    val id: String,
    @SerialName("document_id") val documentId: String = "",
    /** The opaque link token; the public URL is built from it by the caller. */
    val token: String = "",
    val permission: String = DocPermission.VIEW,
    @SerialName("expires_at") val expiresAt: String? = null,
    @SerialName("created_by") val createdBy: String = "",
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("revoked_at") val revokedAt: String? = null,
)

@Serializable
data class ShareListResponse(
    val shares: List<ShareDto> = emptyList(),
)

@Serializable
data class ShareResponse(
    val share: ShareDto,
)

/**
 * Body for `POST /documents/:doc_id/shares`. The instance policy may narrow the
 * permission and cap/force `expires_at`, so always read the values back from the
 * returned [ShareDto] instead of echoing what was requested.
 */
@Serializable
data class CreateShareBody(
    val permission: String? = null,
    /** RFC 3339 instant, e.g. "2026-12-31T23:59:59Z". */
    @SerialName("expires_at") val expiresAt: String? = null,
)

// ─────────────────────────────────────────────────────────────────────────────
// Collaborators & recipients
// ─────────────────────────────────────────────────────────────────────────────

/** A user that can be shared with (`GET /recipients?q=`), and the document owner. */
@Serializable
data class RecipientDto(
    val id: String,
    @SerialName("display_name") val displayName: String? = null,
    val email: String = "",
    @SerialName("avatar_url") val avatarUrl: String? = null,
) {
    /** What to show in a row: the name when the account has one, else the address. */
    val label: String get() = displayName?.takeIf { it.isNotBlank() } ?: email
}

@Serializable
data class RecipientListResponse(
    val recipients: List<RecipientDto> = emptyList(),
)

@Serializable
data class CollaboratorDto(
    @SerialName("user_id") val userId: String,
    val permission: String = DocPermission.VIEW,
    @SerialName("display_name") val displayName: String? = null,
    val email: String = "",
    @SerialName("avatar_url") val avatarUrl: String? = null,
) {
    val label: String get() = displayName?.takeIf { it.isNotBlank() } ?: email
}

/** `GET /documents/:doc_id/collaborators` → `{ owner, collaborators }`. */
@Serializable
data class CollaboratorListResponse(
    val owner: RecipientDto? = null,
    val collaborators: List<CollaboratorDto> = emptyList(),
)

/** Body for `POST /documents/:doc_id/collaborators` (defaults to "edit" server-side). */
@Serializable
data class AddCollaboratorBody(
    @SerialName("user_id") val userId: String,
    val permission: String? = null,
)

/** `POST /documents/:doc_id/collaborators` → `{ ok, user_id, permission }`. */
@Serializable
data class AddCollaboratorResponse(
    val ok: Boolean = false,
    @SerialName("user_id") val userId: String? = null,
    val permission: String? = null,
)

/** Body for `PATCH /documents/:doc_id/collaborators/:user_id`. */
@Serializable
data class UpdateCollaboratorBody(
    val permission: String,
)

/** The three permission levels accepted by shares and collaborators. */
object DocPermission {
    const val VIEW = "view"
    const val COMMENT = "comment"
    const val EDIT = "edit"

    /** True when the level allows writing the document body. */
    fun canEdit(permission: String?): Boolean = permission == EDIT

    /** True when the level allows adding comments (edit implies comment). */
    fun canComment(permission: String?): Boolean = permission == COMMENT || permission == EDIT
}

// ─────────────────────────────────────────────────────────────────────────────
// Templates
// ─────────────────────────────────────────────────────────────────────────────

@Serializable
data class TemplateDto(
    val id: String,
    val name: String = "",
    val description: String? = null,
    val category: String = "general",
    val icon: String? = null,
    @SerialName("content_json") val contentJson: JsonElement? = null,
    /** Built-in templates ship with the module and cannot be deleted. */
    @SerialName("is_builtin") val isBuiltin: Boolean = false,
    @SerialName("created_by") val createdBy: String? = null,
    @SerialName("created_at") val createdAt: String? = null,
)

@Serializable
data class TemplateListResponse(
    val templates: List<TemplateDto> = emptyList(),
)

@Serializable
data class TemplateResponse(
    val template: TemplateDto,
)

/** Body for `POST /documents/templates`. */
@Serializable
data class CreateTemplateBody(
    val name: String,
    val description: String? = null,
    val category: String? = null,
    val icon: String? = null,
    @SerialName("content_json") val contentJson: JsonElement,
)

// ─────────────────────────────────────────────────────────────────────────────
// Editing session (draft + presence)
// ─────────────────────────────────────────────────────────────────────────────

/** One active editor, as reported by `editing/join`. */
@Serializable
data class EditorPresenceDto(
    @SerialName("user_id") val userId: String,
    @SerialName("display_name") val displayName: String? = null,
    /** Server-assigned caret/highlight color, as a CSS hex string. */
    val color: String = "",
    @SerialName("last_ping_at") val lastPingAt: String? = null,
)

/**
 * `POST /documents/:id/editing/join` → `{ content_json, editors }`.
 * The content comes from the DRAFT file, which join creates if it is missing —
 * so this, not `GET /documents/:id`, is the content to edit against.
 */
@Serializable
data class JoinEditingResponse(
    @SerialName("content_json") val contentJson: JsonElement? = null,
    val editors: List<EditorPresenceDto> = emptyList(),
)

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The module's uniform error body. Branch UI logic on [error] (a stable code),
 * never on [message] (French prose meant for display only).
 */
@Serializable
data class ApiErrorDto(
    val error: String = DocErrorCode.INTERNAL,
    val message: String = "",
)

/** Values of [ApiErrorDto.error], with the HTTP status the module pairs them with. */
object DocErrorCode {
    /** 401 — no valid session. */
    const val UNAUTHORIZED = "UNAUTHORIZED"

    /** 403 — authenticated but not allowed on this document. */
    const val FORBIDDEN = "FORBIDDEN"

    /** 403 — the feature is switched off instance-wide (e.g. public share links). */
    const val POLICY_DISABLED = "POLICY_DISABLED"

    /** 404 — unknown document, or one we are not allowed to know exists. */
    const val NOT_FOUND = "NOT_FOUND"

    /** 422 — invalid input (also raised by save-source on a `.doc` origin). */
    const val VALIDATION = "VALIDATION"

    /** 409 — conflicting state. */
    const val CONFLICT = "CONFLICT"

    /** 412 — the `If-Match` etag no longer matches: re-read before rewriting. */
    const val PRECONDITION_FAILED = "PRECONDITION_FAILED"

    /** 422 — the import/export converter could not handle the file. */
    const val CONVERSION_ERROR = "CONVERSION_ERROR"

    const val DATABASE_ERROR = "DATABASE_ERROR"
    const val INTERNAL = "INTERNAL_ERROR"
}
