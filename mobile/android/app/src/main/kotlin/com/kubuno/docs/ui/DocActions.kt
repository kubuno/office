package com.kubuno.docs.ui

import android.content.Context
import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.DeleteForever
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.LinkOff
import androidx.compose.material.icons.filled.OpenInNew
import androidx.compose.material.icons.filled.PersonAdd
import androidx.compose.material.icons.filled.Restore
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.StarBorder
import androidx.compose.material.icons.outlined.Link
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.kubuno.android.account.SharedAccount
import com.kubuno.android.ui.components.KubunoBadge
import com.kubuno.android.ui.components.KubunoBadgeVariant
import com.kubuno.android.ui.components.KubunoButton
import com.kubuno.android.ui.components.KubunoButtonSize
import com.kubuno.android.ui.components.KubunoButtonVariant
import com.kubuno.android.ui.components.KubunoCallout
import com.kubuno.android.ui.components.KubunoCalloutVariant
import com.kubuno.android.ui.components.KubunoChip
import com.kubuno.android.ui.components.KubunoEmptyState
import com.kubuno.android.ui.components.KubunoEmptyTone
import com.kubuno.android.ui.components.KubunoListRow
import com.kubuno.android.ui.components.KubunoSeparator
import com.kubuno.android.ui.components.KubunoSpinner
import com.kubuno.android.ui.components.KubunoSpinnerSize
import com.kubuno.android.ui.components.KubunoTab
import com.kubuno.android.ui.components.KubunoTabs
import com.kubuno.android.ui.components.KubunoTabsVariant
import com.kubuno.android.ui.components.KubunoTextField
import com.kubuno.android.ui.format.formatModified
import com.kubuno.docs.net.AddCollaboratorBody
import com.kubuno.docs.net.ApiErrorDto
import com.kubuno.docs.net.CollaboratorDto
import com.kubuno.docs.net.CreateShareBody
import com.kubuno.docs.net.DocErrorCode
import com.kubuno.docs.net.DocPermission
import com.kubuno.docs.net.DocsApi
import com.kubuno.docs.net.DocsClients
import com.kubuno.docs.net.DocumentSummaryDto
import com.kubuno.docs.net.RecipientDto
import com.kubuno.docs.net.ShareDto
import com.kubuno.docs.net.UpdateCollaboratorBody
import coil3.compose.AsyncImage
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import java.io.IOException
import java.time.Instant
import java.time.temporal.ChronoUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import retrofit2.HttpException

/**
 * Everything that can be done *to* a document: the action sheet of a row (open,
 * rename, star, duplicate, export, trash, restore, delete for good) and the
 * sharing surface (people with access, and public links).
 *
 * The sheet only *names* the action it was tapped for; the confirmation prompts
 * belong to the screen that owns the document, so that each action has exactly
 * one dialog.
 *
 * The sharing half talks to the office API directly instead of going through
 * [DocsViewModel]: collaborators and links are a short-lived panel, and putting
 * them in the single app-wide state would keep lists alive that only matter
 * while the sheet is open.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Targets
// ─────────────────────────────────────────────────────────────────────────────

/** The document an action applies to, reduced to what the menus need. */
data class DocActionTarget(
    val id: String,
    val title: String,
    val isStarred: Boolean = false,
    val isTrashed: Boolean = false,
) {
    // The module allows an untitled document, so the menus need the same stand-in
    // the listing uses (the web's `common_untitled`).
    val displayTitle: String get() = title.trim().ifEmpty { "Sans titre" }
}

fun DocumentSummaryDto.asActionTarget(): DocActionTarget =
    DocActionTarget(id = id, title = title, isStarred = isStarred, isTrashed = isTrashed)

/** One entry of [DocActionsSheet]. */
enum class DocMenuAction {
    OPEN,
    RENAME,
    TOGGLE_STAR,
    SHARE,
    DUPLICATE,
    EXPORT,
    TRASH,
    RESTORE,
    DELETE_FOREVER,
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The action menu of one document row. Draws nothing while [expanded] is false.
 *
 * A bottom sheet rather than an anchored dropdown: the list is long enough to
 * reach a phone's screen edge, and every action here is a one-handed tap.
 */
@Composable
fun DocActions(
    document: DocumentSummaryDto,
    expanded: Boolean,
    onDismiss: () -> Unit,
    onOpen: () -> Unit,
    onRename: () -> Unit,
    onToggleStar: () -> Unit,
    onDuplicate: () -> Unit,
    onShare: () -> Unit,
    onExport: () -> Unit,
    onTrash: () -> Unit,
    onRestore: () -> Unit,
    onDeleteForever: () -> Unit,
) {
    if (!expanded) return
    DocActionsSheet(
        target = document.asActionTarget(),
        onDismiss = onDismiss,
        onAction = { action ->
            when (action) {
                DocMenuAction.OPEN -> onOpen()
                DocMenuAction.RENAME -> onRename()
                DocMenuAction.TOGGLE_STAR -> onToggleStar()
                DocMenuAction.DUPLICATE -> onDuplicate()
                DocMenuAction.SHARE -> onShare()
                DocMenuAction.EXPORT -> onExport()
                DocMenuAction.TRASH -> onTrash()
                DocMenuAction.RESTORE -> onRestore()
                DocMenuAction.DELETE_FOREVER -> onDeleteForever()
            }
        },
    )
}

/**
 * The action list itself, over a [DocActionTarget] so any screen holding a
 * document can reuse it without depending on the listing's row type.
 *
 * A trashed document only gets restore and permanent deletion: the module
 * refuses the rest on a trashed row anyway.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DocActionsSheet(
    target: DocActionTarget,
    onAction: (DocMenuAction) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp)
                .padding(bottom = 8.dp)
                .navigationBarsPadding(),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            DocSheetHeader(title = target.displayTitle)

            if (target.isTrashed) {
                DocActionRow(
                    label = "Restaurer",
                    icon = { Icon(Icons.Filled.Restore, contentDescription = null) },
                    onClick = { onAction(DocMenuAction.RESTORE) },
                )
                KubunoSeparator(Modifier.padding(vertical = 4.dp))
                DocActionRow(
                    label = "Supprimer définitivement",
                    icon = {
                        Icon(
                            Icons.Filled.DeleteForever,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.error,
                        )
                    },
                    onClick = { onAction(DocMenuAction.DELETE_FOREVER) },
                )
            } else {
                DocActionRow(
                    label = "Ouvrir",
                    icon = { Icon(Icons.Filled.OpenInNew, contentDescription = null) },
                    onClick = { onAction(DocMenuAction.OPEN) },
                )
                DocActionRow(
                    label = "Renommer",
                    icon = { Icon(Icons.Filled.Edit, contentDescription = null) },
                    onClick = { onAction(DocMenuAction.RENAME) },
                )
                DocActionRow(
                    label = if (target.isStarred) "Retirer des favoris" else "Ajouter aux favoris",
                    icon = {
                        Icon(
                            if (target.isStarred) Icons.Filled.Star else Icons.Filled.StarBorder,
                            contentDescription = null,
                            tint = if (target.isStarred) {
                                docsTone()
                            } else {
                                MaterialTheme.colorScheme.onSurfaceVariant
                            },
                        )
                    },
                    onClick = { onAction(DocMenuAction.TOGGLE_STAR) },
                )
                DocActionRow(
                    label = "Partager",
                    icon = { Icon(Icons.Filled.Share, contentDescription = null) },
                    onClick = { onAction(DocMenuAction.SHARE) },
                )
                DocActionRow(
                    label = "Dupliquer",
                    icon = { Icon(Icons.Filled.ContentCopy, contentDescription = null) },
                    onClick = { onAction(DocMenuAction.DUPLICATE) },
                )
                DocActionRow(
                    label = "Exporter",
                    icon = { Icon(Icons.Filled.Download, contentDescription = null) },
                    onClick = { onAction(DocMenuAction.EXPORT) },
                )
                KubunoSeparator(Modifier.padding(vertical = 4.dp))
                DocActionRow(
                    label = "Mettre à la corbeille",
                    icon = {
                        Icon(
                            Icons.Filled.Delete,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.error,
                        )
                    },
                    onClick = { onAction(DocMenuAction.TRASH) },
                )
            }
        }
    }
}

@Composable
private fun DocActionRow(
    label: String,
    icon: @Composable () -> Unit,
    onClick: () -> Unit,
) {
    KubunoListRow(
        title = label,
        modifier = Modifier.fillMaxWidth(),
        onClick = onClick,
        leading = icon,
    )
}

@Composable
private fun DocSheetHeader(title: String, subtitle: String? = null) {
    Column(Modifier.padding(horizontal = 4.dp, vertical = 8.dp)) {
        Text(
            title,
            style = MaterialTheme.typography.titleMedium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (subtitle != null) {
            Text(
                subtitle,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sharing
// ─────────────────────────────────────────────────────────────────────────────

private const val SHARE_TAB_PEOPLE = "people"
private const val SHARE_TAB_LINKS = "links"

/** How long a new public link stays valid. */
enum class DocShareExpiry(val days: Long, val label: String) {
    NEVER(0, "Sans expiration"),
    WEEK(7, "7 jours"),
    MONTH(30, "30 jours"),
    QUARTER(90, "90 jours"),
    ;

    /**
     * RFC 3339 instant for the API, or null for "no expiry". The instance can
     * still shorten or impose one, so the created share's own `expires_at` is
     * what gets displayed, never this request value.
     */
    fun toExpiresAt(): String? =
        if (days <= 0L) null else Instant.now().plus(days, ChronoUnit.DAYS).toString()
}

/** French label of a `view` / `comment` / `edit` permission. */
fun docPermissionLabel(permission: String?): String = when (permission) {
    DocPermission.EDIT -> "Modification"
    DocPermission.COMMENT -> "Commentaire"
    else -> "Lecture"
}

/**
 * The public endpoint that serves the document behind [token].
 *
 * This is the module's own public route (`/api/v1/office/public/:token`); the
 * office web front registers no page route for a shared document, so anything
 * else here would be a URL invented on the client.
 */
fun documentShareLinkUrl(account: SharedAccount, token: String): String =
    "${account.serverUrl.trimEnd('/')}/api/v1/office/public/$token"

/**
 * The sharing surface: who has access, and the public links.
 *
 * It owns its loading and error state because nothing here outlives the sheet.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DocShareSheet(
    documentId: String,
    documentTitle: String,
    account: SharedAccount?,
    onDismiss: () -> Unit,
    api: DocsApi? = rememberDocsApi(account),
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var tab by remember { mutableStateOf(SHARE_TAB_PEOPLE) }

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheetState) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp)
                .padding(bottom = 16.dp)
                .navigationBarsPadding(),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            DocSheetHeader(title = "Partager", subtitle = documentTitle)

            KubunoTabs(
                tabs = listOf(
                    KubunoTab(SHARE_TAB_PEOPLE, "Personnes"),
                    KubunoTab(SHARE_TAB_LINKS, "Liens"),
                ),
                selectedId = tab,
                onSelect = { tab = it },
                variant = KubunoTabsVariant.UNDERLINED,
            )

            if (api == null || account == null) {
                KubunoCallout(
                    text = "Aucun compte Kubuno disponible : le partage est indisponible.",
                    variant = KubunoCalloutVariant.WARNING,
                )
            } else if (tab == SHARE_TAB_PEOPLE) {
                DocSharePeoplePanel(documentId = documentId, api = api, account = account)
            } else {
                DocShareLinksPanel(documentId = documentId, api = api, account = account)
            }
        }
    }
}

/** Owner, collaborators, and the recipient search that adds one. */
@Composable
fun DocSharePeoplePanel(
    documentId: String,
    api: DocsApi,
    account: SharedAccount,
) {
    val scope = rememberCoroutineScope()

    var loading by remember(documentId) { mutableStateOf(true) }
    var owner by remember(documentId) { mutableStateOf<RecipientDto?>(null) }
    var collaborators by remember(documentId) { mutableStateOf<List<CollaboratorDto>>(emptyList()) }
    var error by remember(documentId) { mutableStateOf<DocsError?>(null) }
    var busy by remember(documentId) { mutableStateOf(false) }

    var query by remember(documentId) { mutableStateOf("") }
    var results by remember(documentId) { mutableStateOf<List<RecipientDto>>(emptyList()) }
    var searching by remember(documentId) { mutableStateOf(false) }
    var permission by remember(documentId) { mutableStateOf(DocPermission.EDIT) }

    val reload: suspend () -> Unit = {
        docApiCall { api.listCollaborators(documentId) }
            .onSuccess {
                owner = it.owner
                collaborators = it.collaborators
                error = null
            }
            .onFailure { error = docActionErrorOf(it) }
        loading = false
    }

    LaunchedEffect(documentId) { reload() }

    // Debounced like the web dialog: without it the recipient endpoint is hit on
    // every keystroke.
    LaunchedEffect(documentId, query) {
        val q = query.trim()
        if (q.isEmpty()) {
            results = emptyList()
            searching = false
            return@LaunchedEffect
        }
        delay(250)
        searching = true
        docApiCall { api.searchRecipients(q) }
            .onSuccess { results = it.recipients }
            .onFailure { error = docActionErrorOf(it) }
        searching = false
    }

    val known = buildSet {
        owner?.id?.let { add(it) }
        collaborators.forEach { add(it.userId) }
    }
    val suggestions = results.filterNot { known.contains(it.id) }

    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        error?.let { KubunoCallout(text = it.message, variant = KubunoCalloutVariant.DANGER) }

        KubunoTextField(
            value = query,
            onValueChange = { query = it },
            modifier = Modifier.fillMaxWidth(),
            placeholder = "Ajouter par nom ou adresse",
            leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null) },
            trailingIcon = { if (searching) KubunoSpinner(size = KubunoSpinnerSize.XS) },
        )

        DocPermissionPicker(value = permission, onChange = { permission = it })

        if (query.isNotBlank()) {
            if (suggestions.isEmpty() && !searching) {
                Text(
                    "Aucun utilisateur",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(max = 200.dp)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    suggestions.forEach { recipient ->
                        DocRecipientRow(
                            recipient = recipient,
                            account = account,
                            enabled = !busy,
                            onAdd = {
                                scope.launch {
                                    busy = true
                                    docApiCall {
                                        api.addCollaborator(
                                            documentId,
                                            AddCollaboratorBody(recipient.id, permission),
                                        )
                                    }
                                        .onSuccess {
                                            query = ""
                                            results = emptyList()
                                            reload()
                                        }
                                        .onFailure { error = docActionErrorOf(it) }
                                    busy = false
                                }
                            },
                        )
                    }
                }
            }
        }

        KubunoSeparator()

        Text(
            "Personnes ayant accès",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        if (loading) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(vertical = 12.dp),
                horizontalArrangement = Arrangement.Center,
            ) { KubunoSpinner(size = KubunoSpinnerSize.SM) }
        } else {
            Column(
                Modifier
                    .fillMaxWidth()
                    .heightIn(max = 280.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                owner?.let { current ->
                    KubunoListRow(
                        title = current.label,
                        modifier = Modifier.fillMaxWidth(),
                        subtitle = current.email.takeIf { it.isNotBlank() && it != current.label },
                        leading = {
                            DocAvatar(
                                id = current.id,
                                name = current.label,
                                account = account,
                                hasAvatar = current.avatarUrl != null,
                            )
                        },
                        trailing = {
                            KubunoBadge(text = "Propriétaire", variant = KubunoBadgeVariant.NEUTRAL)
                        },
                    )
                }

                if (collaborators.isEmpty()) {
                    Text(
                        "Vous seul avez accès à ce document.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }

                collaborators.forEach { collaborator ->
                    DocCollaboratorRow(
                        collaborator = collaborator,
                        account = account,
                        enabled = !busy,
                        onPermissionChange = { next ->
                            scope.launch {
                                busy = true
                                docApiCall {
                                    api.updateCollaborator(
                                        documentId,
                                        collaborator.userId,
                                        UpdateCollaboratorBody(next),
                                    )
                                }
                                    .onSuccess { reload() }
                                    .onFailure { error = docActionErrorOf(it) }
                                busy = false
                            }
                        },
                        onRemove = {
                            scope.launch {
                                busy = true
                                docApiCall { api.removeCollaborator(documentId, collaborator.userId) }
                                    .onSuccess { reload() }
                                    .onFailure { error = docActionErrorOf(it) }
                                busy = false
                            }
                        },
                    )
                }
            }
        }
    }
}

/** Existing public links, creation of a new one, and revocation. */
@Composable
fun DocShareLinksPanel(
    documentId: String,
    api: DocsApi,
    account: SharedAccount,
) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current

    var loading by remember(documentId) { mutableStateOf(true) }
    var shares by remember(documentId) { mutableStateOf<List<ShareDto>>(emptyList()) }
    var error by remember(documentId) { mutableStateOf<DocsError?>(null) }
    var busy by remember(documentId) { mutableStateOf(false) }
    // Listing links works even when the instance forbids creating them, so the
    // policy only shows up on a create attempt — hence a flag, not a preflight.
    var policyDisabled by remember(documentId) { mutableStateOf(false) }

    var permission by remember(documentId) { mutableStateOf(DocPermission.VIEW) }
    var expiry by remember(documentId) { mutableStateOf(DocShareExpiry.NEVER) }

    val reload: suspend () -> Unit = {
        docApiCall { api.listShares(documentId) }
            .onSuccess {
                shares = it.shares
                error = null
            }
            .onFailure { error = docActionErrorOf(it) }
        loading = false
    }

    LaunchedEffect(documentId) { reload() }

    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (policyDisabled) {
            KubunoCallout(
                text = "L'administrateur de cette instance a désactivé les liens de partage public. " +
                    "Ajoutez plutôt des personnes dans l'onglet « Personnes ».",
                variant = KubunoCalloutVariant.WARNING,
                title = "Liens publics désactivés",
            )
        }
        error?.takeIf { it.code != DocErrorCode.POLICY_DISABLED }?.let {
            KubunoCallout(text = it.message, variant = KubunoCalloutVariant.DANGER)
        }

        if (!policyDisabled) {
            Text(
                "Nouveau lien",
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            DocPermissionPicker(value = permission, onChange = { permission = it })
            DocShareExpiryPicker(value = expiry, onChange = { expiry = it })
            KubunoButton(
                text = "Créer le lien",
                onClick = {
                    scope.launch {
                        busy = true
                        docApiCall {
                            api.createShare(
                                documentId,
                                CreateShareBody(
                                    permission = permission,
                                    expiresAt = expiry.toExpiresAt(),
                                ),
                            )
                        }
                            .onSuccess { reload() }
                            .onFailure {
                                val mapped = docActionErrorOf(it)
                                error = mapped
                                policyDisabled = mapped.code == DocErrorCode.POLICY_DISABLED
                            }
                        busy = false
                    }
                },
                size = KubunoButtonSize.SM,
                enabled = !busy,
                loading = busy,
                icon = { Icon(Icons.Filled.Link, contentDescription = null) },
            )
        }

        KubunoSeparator()

        if (loading) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(vertical = 12.dp),
                horizontalArrangement = Arrangement.Center,
            ) { KubunoSpinner(size = KubunoSpinnerSize.SM) }
        } else if (shares.isEmpty()) {
            KubunoEmptyState(
                icon = Icons.Outlined.Link,
                title = "Aucun lien",
                modifier = Modifier.fillMaxWidth(),
                description = "Ce document n'est accessible qu'aux personnes invitées.",
                tone = KubunoEmptyTone.FIRST_USE,
                compact = true,
            )
        } else {
            Column(
                Modifier
                    .fillMaxWidth()
                    .heightIn(max = 300.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                shares.forEach { share ->
                    val url = documentShareLinkUrl(account, share.token)
                    DocShareLinkRow(
                        share = share,
                        url = url,
                        enabled = !busy,
                        onCopy = { clipboard.setText(AnnotatedString(url)) },
                        onSend = { shareLinkWithSystem(context, url) },
                        onRevoke = {
                            scope.launch {
                                busy = true
                                docApiCall { api.revokeShare(documentId, share.id) }
                                    .onSuccess { reload() }
                                    .onFailure { error = docActionErrorOf(it) }
                                busy = false
                            }
                        },
                    )
                }
            }
        }
    }
}

/** The three permission levels, as a chip row. */
@Composable
fun DocPermissionPicker(
    value: String,
    onChange: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        listOf(DocPermission.VIEW, DocPermission.COMMENT, DocPermission.EDIT).forEach { level ->
            KubunoChip(
                label = docPermissionLabel(level),
                selected = value == level,
                onClick = { onChange(level) },
            )
        }
    }
}

/** Lifetime of the link about to be created. */
@Composable
fun DocShareExpiryPicker(
    value: DocShareExpiry,
    onChange: (DocShareExpiry) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        DocShareExpiry.entries.forEach { option ->
            KubunoChip(
                label = option.label,
                selected = value == option,
                onClick = { onChange(option) },
            )
        }
    }
}

@Composable
fun DocRecipientRow(
    recipient: RecipientDto,
    account: SharedAccount,
    enabled: Boolean,
    onAdd: () -> Unit,
) {
    KubunoListRow(
        title = recipient.label,
        modifier = Modifier.fillMaxWidth(),
        subtitle = recipient.email.takeIf { it.isNotBlank() && it != recipient.label },
        onClick = if (enabled) onAdd else null,
        leading = {
            DocAvatar(
                id = recipient.id,
                name = recipient.label,
                account = account,
                hasAvatar = recipient.avatarUrl != null,
            )
        },
        trailing = { Icon(Icons.Filled.PersonAdd, contentDescription = "Ajouter") },
    )
}

@Composable
fun DocCollaboratorRow(
    collaborator: CollaboratorDto,
    account: SharedAccount,
    enabled: Boolean,
    onPermissionChange: (String) -> Unit,
    onRemove: () -> Unit,
) {
    Column(Modifier.fillMaxWidth()) {
        KubunoListRow(
            title = collaborator.label,
            modifier = Modifier.fillMaxWidth(),
            subtitle = collaborator.email.takeIf { it.isNotBlank() && it != collaborator.label },
            leading = {
                DocAvatar(
                    id = collaborator.userId,
                    name = collaborator.label,
                    account = account,
                    hasAvatar = collaborator.avatarUrl != null,
                )
            },
            trailing = {
                IconButton(onClick = onRemove, enabled = enabled) {
                    Icon(
                        Icons.Filled.Delete,
                        contentDescription = "Retirer",
                        tint = MaterialTheme.colorScheme.error,
                    )
                }
            },
        )
        DocPermissionPicker(
            value = collaborator.permission,
            onChange = { if (enabled) onPermissionChange(it) },
            modifier = Modifier.padding(start = 48.dp, bottom = 4.dp),
        )
    }
}

@Composable
fun DocShareLinkRow(
    share: ShareDto,
    url: String,
    enabled: Boolean,
    onCopy: () -> Unit,
    onSend: () -> Unit,
    onRevoke: () -> Unit,
) {
    val expiry = formatModified(share.expiresAt)
    Column(Modifier.fillMaxWidth()) {
        KubunoListRow(
            title = docPermissionLabel(share.permission),
            modifier = Modifier.fillMaxWidth(),
            subtitle = if (expiry != null) "Expire le $expiry" else "Sans expiration",
            leading = { Icon(Icons.Filled.Link, contentDescription = null) },
            trailing = {
                IconButton(onClick = onRevoke, enabled = enabled) {
                    Icon(
                        Icons.Filled.LinkOff,
                        contentDescription = "Révoquer",
                        tint = MaterialTheme.colorScheme.error,
                    )
                }
            },
        )
        Text(
            url,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(start = 48.dp),
        )
        Row(
            Modifier.padding(start = 44.dp, top = 2.dp, bottom = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            KubunoButton(
                text = "Copier",
                onClick = onCopy,
                variant = KubunoButtonVariant.TEXT,
                size = KubunoButtonSize.SM,
                icon = { Icon(Icons.Filled.ContentCopy, contentDescription = null) },
            )
            KubunoButton(
                text = "Envoyer",
                onClick = onSend,
                variant = KubunoButtonVariant.TEXT,
                size = KubunoButtonSize.SM,
                icon = { Icon(Icons.Filled.Share, contentDescription = null) },
            )
        }
    }
}

/**
 * A round avatar: the instance picture drawn over the initials, so a fetch that
 * fails or is refused degrades to the letters instead of a hole.
 */
@Composable
fun DocAvatar(
    id: String,
    name: String,
    account: SharedAccount,
    hasAvatar: Boolean,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier
            .size(32.dp)
            .clip(CircleShape)
            .background(docAvatarColor(id)),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            docInitials(name),
            style = MaterialTheme.typography.labelSmall,
            color = Color.White,
            fontWeight = FontWeight.SemiBold,
        )
        if (hasAvatar) {
            AsyncImage(
                model = docsUserAvatarUrl(account, id),
                contentDescription = null,
                modifier = Modifier
                    .size(32.dp)
                    .clip(CircleShape),
            )
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Clients
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The office API for [account], or null when there is no account.
 *
 * Resolved through a Hilt entry point so a screen can drop the share sheet in
 * without plumbing the singleton down from the activity. The instance is the
 * very one the view model uses, brokered token cache included.
 */
@Composable
fun rememberDocsApi(account: SharedAccount?): DocsApi? {
    val context = LocalContext.current
    return remember(account) { account?.let { docsClientsOf(context).api(it) } }
}

@EntryPoint
@InstallIn(SingletonComponent::class)
internal interface DocActionsClientsEntryPoint {
    fun docsClients(): DocsClients
}

private fun docsClientsOf(context: Context): DocsClients =
    EntryPointAccessors
        .fromApplication(context.applicationContext, DocActionsClientsEntryPoint::class.java)
        .docsClients()

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/** Hands the link to the system share targets. */
private fun shareLinkWithSystem(context: Context, url: String) {
    val intent = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_TEXT, url)
    }
    context.startActivity(Intent.createChooser(intent, "Partager le lien"))
}

/** CollaboratorsDialog.tsx initials(): one letter per end of the name, at most two. */
private fun docInitials(name: String): String {
    val parts = name.trim().split(Regex("\\s+")).filter { it.isNotEmpty() }
    return when {
        parts.isEmpty() -> "?"
        parts.size == 1 -> parts[0].take(2).uppercase()
        else -> (parts.first().take(1) + parts.last().take(1)).uppercase()
    }
}

/** CollaboratorsDialog.tsx colorFor(): the same palette, in the same order. */
private val DOC_AVATAR_PALETTE = listOf(
    Color(0xFF1A73E8), Color(0xFFD93025), Color(0xFF1E8E3E), Color(0xFFF9AB00),
    Color(0xFF9334E6), Color(0xFFE8710A), Color(0xFF12B5CB), Color(0xFFD01884),
)

private fun docAvatarColor(id: String): Color {
    var hash = 0
    for (ch in id) hash = hash * 31 + ch.code
    // The web hashes into an unsigned 32-bit slot (`>>> 0`); Kotlin's Int is
    // signed, so the sign bit is masked off to land in the same bucket.
    val index = (hash.toLong() and 0xFFFFFFFFL) % DOC_AVATAR_PALETTE.size
    return DOC_AVATAR_PALETTE[index.toInt()]
}

/** Runs one API call off the main thread and never throws. */
private suspend fun <T> docApiCall(block: suspend () -> T): Result<T> =
    runCatching { withContext(Dispatchers.IO) { block() } }

private val docActionJson = Json { ignoreUnknownKeys = true }

/**
 * The same mapping the view model uses: the code comes from the module's
 * `{ "error": … }` envelope, the HTTP status is only the fallback for a body
 * that is missing or not ours.
 */
private fun docActionErrorOf(t: Throwable): DocsError = when (t) {
    is HttpException -> {
        val body = runCatching { t.response()?.errorBody()?.string() }.getOrNull()
        val code = body
            ?.let {
                runCatching {
                    docActionJson.decodeFromString(ApiErrorDto.serializer(), it).error
                }.getOrNull()
            }
            ?: docActionStatusCode(t.code())
        DocsError(code, docActionMessage(code))
    }
    is IOException -> DocsError(DocsError.NETWORK, docActionMessage(DocsError.NETWORK))
    else -> DocsError(DocErrorCode.INTERNAL, docActionMessage(DocErrorCode.INTERNAL))
}

private fun docActionStatusCode(status: Int): String = when (status) {
    401 -> DocErrorCode.UNAUTHORIZED
    403 -> DocErrorCode.FORBIDDEN
    404 -> DocErrorCode.NOT_FOUND
    409 -> DocErrorCode.CONFLICT
    412 -> DocErrorCode.PRECONDITION_FAILED
    422 -> DocErrorCode.VALIDATION
    else -> DocErrorCode.INTERNAL
}

private fun docActionMessage(code: String): String = when (code) {
    DocErrorCode.UNAUTHORIZED ->
        "Session expirée. Reconnectez-vous depuis l'application Kubuno Drive."
    DocErrorCode.FORBIDDEN -> "Vous n'avez pas les droits sur ce document."
    DocErrorCode.POLICY_DISABLED -> "Cette fonction est désactivée sur ce serveur."
    DocErrorCode.NOT_FOUND -> "Élément introuvable."
    DocErrorCode.VALIDATION -> "Demande invalide."
    DocErrorCode.CONFLICT -> "Opération impossible dans l'état actuel."
    DocErrorCode.PRECONDITION_FAILED -> "Le document a été modifié ailleurs."
    DocErrorCode.CONVERSION_ERROR -> "Ce format n'a pas pu être converti."
    DocErrorCode.DATABASE_ERROR, DocErrorCode.INTERNAL -> "Erreur du serveur. Réessayez."
    DocsError.NETWORK -> "Connexion impossible. Vérifiez votre réseau."
    DocsError.NO_ACCOUNT -> "Aucun compte Kubuno sur cet appareil."
    else -> "Une erreur est survenue."
}
