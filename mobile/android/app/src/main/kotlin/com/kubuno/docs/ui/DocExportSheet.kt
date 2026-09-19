package com.kubuno.docs.ui

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.FileDownload
import androidx.compose.material.icons.outlined.OpenInNew
import androidx.compose.material.icons.outlined.Save
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import com.kubuno.android.account.SharedAccount
import com.kubuno.android.ui.components.KubunoButton
import com.kubuno.android.ui.components.KubunoButtonSize
import com.kubuno.android.ui.components.KubunoButtonVariant
import com.kubuno.android.ui.components.KubunoCallout
import com.kubuno.android.ui.components.KubunoCalloutVariant
import com.kubuno.android.ui.components.KubunoCard
import com.kubuno.android.ui.components.KubunoListRow
import com.kubuno.android.ui.components.KubunoProgressBar
import com.kubuno.android.ui.components.KubunoSeparator
import com.kubuno.docs.net.ApiErrorDto
import com.kubuno.docs.net.DocErrorCode
import com.kubuno.docs.net.DocsClients
import com.kubuno.docs.net.DocumentDto
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import java.io.File
import java.io.IOException
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import okhttp3.Request
import retrofit2.HttpException

// ─────────────────────────────────────────────────────────────────────────────
// Public surface
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The export sheet of an open document: the two binary formats the module can
 * produce (DOCX, ODT) and, for a document that came from an imported file, the
 * "write it back into its source file" action.
 *
 * Mirrors the export panel of the web backstage
 * (office/frontend/src/DocumentsBackstage.tsx) minus one entry: the web's PDF is
 * rasterised in the browser from the paginated page canvas
 * (`exportPageCanvases()`), an engine this app deliberately does not carry — the
 * phone reflows the document instead of paginating it (see [DocsPage]). There is
 * no server-side PDF endpoint to fall back on, so no PDF option is shown at all
 * rather than one that would always fail. The plain-text entry is left out for
 * the same "nothing that cannot be honoured" reason: it is produced from the
 * editor's own text model on the web.
 *
 * The download runs on this sheet's own composition scope, not in the view
 * model: leaving the sheet cancels a transfer the user walked away from, and the
 * file only ever reaches the cache folders declared in `res/xml/file_paths.xml`.
 *
 * @param document the OPEN document — [DocumentDto.sourceFormat] decides whether
 *   the save-to-source row is offered at all.
 * @param dirty true when the editor holds unsaved changes; the server exports
 *   the stored revision, so the sheet warns instead of exporting stale bytes
 *   silently.
 * @param clients resolved from the Hilt singleton component by default, so a
 *   screen that does not inject it can still show the sheet.
 * @param onMessage a short French line worth raising as a snackbar by the host
 *   screen; the sheet also shows it inline.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DocExportSheet(
    document: DocumentDto,
    account: SharedAccount,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    dirty: Boolean = false,
    clients: DocsClients = rememberDocsClients(),
    onMessage: (String) -> Unit = {},
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    // Keyed on the document: reopening the sheet on another document must not
    // inherit the previous one's downloaded file or error.
    var phase by remember(document.id) { mutableStateOf<ExportPhase>(ExportPhase.Idle) }
    var notice by remember(document.id) { mutableStateOf<ExportNotice?>(null) }

    // Progress is written from the IO thread that copies the stream, which is why
    // it lives in a flow rather than in a snapshot state written off the main
    // thread. StateFlow conflates, so a fast transfer cannot flood recomposition.
    val progressFlow = remember(document.id) { MutableStateFlow<Float?>(null) }
    val progress by progressFlow.collectAsState()

    val busy = phase is ExportPhase.Downloading || phase == ExportPhase.SavingSource

    fun export(format: DocsExportFormat) {
        notice = null
        progressFlow.value = null
        phase = ExportPhase.Downloading(format)
        scope.launch {
            val result = runCatching {
                downloadExport(context, clients, account, document, format, progressFlow)
            }
            result.onSuccess { file ->
                phase = ExportPhase.Ready(file, format)
            }.onFailure { failure ->
                phase = ExportPhase.Idle
                notice = exportNoticeOf(failure)
            }
        }
    }

    fun saveToSource() {
        notice = null
        // A `.doc` origin has no writer server-side (the module's WRITABLE set is
        // docx|odt), so the call would come back 422 with the same advice. Answer
        // it locally instead of spending a round trip to render the same hint.
        if (!document.canSaveToSource) {
            notice = DocOriginNotice
            return
        }
        phase = ExportPhase.SavingSource
        scope.launch {
            val result = runCatching {
                withContext(Dispatchers.IO) { clients.api(account).saveToSource(document.id) }
            }
            phase = ExportPhase.Idle
            result.onSuccess { response ->
                val written = (response.format ?: document.sourceFormat).orEmpty().uppercase(Locale.ROOT)
                val text = if (written.isEmpty()) {
                    "Fichier d'origine mis à jour."
                } else {
                    "Fichier d'origine mis à jour ($written)."
                }
                notice = ExportNotice(KubunoCalloutVariant.SUCCESS, text)
                onMessage(text)
            }.onFailure { failure ->
                // 422 on this endpoint means "this origin cannot be rewritten" far
                // more often than it means a broken document, so it gets the
                // actionable hint rather than the raw server sentence.
                notice = if (isUnwritableOrigin(failure)) DocOriginNotice else exportNoticeOf(failure)
            }
        }
    }

    fun openReady(ready: ExportPhase.Ready) {
        val uri = FileProvider.getUriForFile(context, fileProviderAuthority(context), ready.file)
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, ready.format.mime)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        try {
            context.startActivity(intent)
        } catch (_: ActivityNotFoundException) {
            notice = ExportNotice(
                KubunoCalloutVariant.WARNING,
                "Aucune application installée ne sait ouvrir ce format. Partagez plutôt le fichier.",
            )
        }
    }

    fun shareReady(ready: ExportPhase.Ready) {
        scope.launch {
            val staged = runCatching {
                withContext(Dispatchers.IO) { stageForShare(context, ready.file) }
            }.getOrNull()
            if (staged == null) {
                notice = ExportNotice(KubunoCalloutVariant.DANGER, "Le fichier n'a pas pu être préparé au partage.")
                return@launch
            }
            val uri = FileProvider.getUriForFile(context, fileProviderAuthority(context), staged)
            val intent = Intent(Intent.ACTION_SEND).apply {
                type = ready.format.mime
                putExtra(Intent.EXTRA_STREAM, uri)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            runCatching { context.startActivity(Intent.createChooser(intent, "Partager")) }
                .onFailure {
                    notice = ExportNotice(KubunoCalloutVariant.WARNING, "Aucune application ne peut recevoir ce fichier.")
                }
        }
    }

    ModalBottomSheet(onDismissRequest = onDismiss, modifier = modifier) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .padding(bottom = 28.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    Icons.Outlined.FileDownload,
                    contentDescription = null,
                    tint = docsTone(),
                    modifier = Modifier.size(22.dp),
                )
                Text(
                    "Exporter",
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.padding(start = 10.dp),
                )
            }
            Text(
                document.title.ifBlank { "Document sans titre" },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 2.dp, bottom = 12.dp),
            )

            if (dirty) {
                KubunoCallout(
                    "Des modifications ne sont pas encore enregistrées : l'export reprend la dernière version enregistrée sur le serveur.",
                    variant = KubunoCalloutVariant.WARNING,
                    modifier = Modifier.padding(bottom = 12.dp),
                )
            }

            notice?.let { shown ->
                KubunoCallout(
                    shown.text,
                    variant = shown.variant,
                    title = shown.title,
                    modifier = Modifier.padding(bottom = 12.dp),
                )
            }

            when (val current = phase) {
                is ExportPhase.Downloading -> KubunoProgressBar(
                    progress = progress,
                    modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp),
                    label = "Export ${current.format.extension.uppercase(Locale.ROOT)} en cours…",
                    showValue = progress != null,
                )

                ExportPhase.SavingSource -> KubunoProgressBar(
                    progress = null,
                    modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp),
                    label = "Enregistrement dans le fichier d'origine…",
                    showValue = false,
                )

                is ExportPhase.Ready -> KubunoCard(
                    modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp),
                    title = current.file.name,
                    subtitle = humanSize(current.file.length()),
                    icon = {
                        Icon(
                            Icons.Outlined.Description,
                            contentDescription = null,
                            tint = docsTone(),
                            modifier = Modifier.size(20.dp),
                        )
                    },
                ) {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        KubunoButton(
                            "Ouvrir",
                            onClick = { openReady(current) },
                            size = KubunoButtonSize.SM,
                            icon = {
                                Icon(Icons.Outlined.OpenInNew, contentDescription = null, modifier = Modifier.size(16.dp))
                            },
                        )
                        KubunoButton(
                            "Partager",
                            onClick = { shareReady(current) },
                            variant = KubunoButtonVariant.SECONDARY,
                            size = KubunoButtonSize.SM,
                            icon = {
                                Icon(Icons.Outlined.Share, contentDescription = null, modifier = Modifier.size(16.dp))
                            },
                        )
                    }
                }

                ExportPhase.Idle -> Unit
            }

            // Shown first, like the web backstage: for a document opened from a
            // file, updating that file is the action the user came for.
            if (document.sourceFormat != null) {
                KubunoListRow(
                    title = "Enregistrer (${saveTargetLabel(document.sourceFormat)})",
                    subtitle = if (document.canSaveToSource) {
                        "Met à jour le fichier d'origine"
                    } else {
                        "Ce document vient d'un .doc, que nous ne savons pas réécrire"
                    },
                    onClick = if (busy) null else ({ saveToSource() }),
                    leading = {
                        Icon(
                            Icons.Outlined.Save,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.size(20.dp),
                        )
                    },
                )
                KubunoSeparator(Modifier.padding(vertical = 8.dp))
            }

            DocsExportFormat.entries.forEach { format ->
                KubunoListRow(
                    title = formatLabel(format),
                    subtitle = formatHint(format),
                    onClick = if (busy) null else ({ export(format) }),
                    leading = {
                        Icon(
                            Icons.Outlined.Description,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.size(20.dp),
                        )
                    },
                )
            }

            Spacer(Modifier.height(12.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                KubunoButton(
                    "Fermer",
                    onClick = onDismiss,
                    variant = KubunoButtonVariant.GHOST,
                    size = KubunoButtonSize.SM,
                )
            }
        }
    }
}

/**
 * The app-wide [DocsClients], for a screen that shows the sheet without having
 * the dependency injected itself. It is a `@Singleton`, so this hands back the
 * very instance the view model uses — including its brokered token cache.
 *
 * Private on purpose: it only exists to feed this sheet's default argument, and
 * a second copy of a generally-named helper would clash with another screen's.
 */
@Composable
private fun rememberDocsClients(): DocsClients {
    val context = LocalContext.current
    return remember(context) {
        EntryPointAccessors
            .fromApplication(context.applicationContext, DocExportClientsEntryPoint::class.java)
            .docsClients()
    }
}

@EntryPoint
@InstallIn(SingletonComponent::class)
internal interface DocExportClientsEntryPoint {
    fun docsClients(): DocsClients
}

// ─────────────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────────────

private sealed interface ExportPhase {
    data object Idle : ExportPhase
    data object SavingSource : ExportPhase
    data class Downloading(val format: DocsExportFormat) : ExportPhase
    data class Ready(val file: File, val format: DocsExportFormat) : ExportPhase
}

/** An inline message, kept next to the actions instead of replacing them. */
private data class ExportNotice(
    val variant: KubunoCalloutVariant,
    val text: String,
    val title: String? = null,
)

/**
 * What a `.doc` origin gets, whether the sheet predicted it or the server said
 * so: an instruction, not the raw refusal.
 */
private val DocOriginNotice = ExportNotice(
    variant = KubunoCalloutVariant.WARNING,
    title = "Format d'origine non réinscriptible",
    text = "Ce document provient d'un .doc, un format que nous savons lire mais pas réécrire. " +
        "Exportez-le en DOCX, puis conservez ce nouveau fichier.",
)

private val ExportJson = Json { ignoreUnknownKeys = true }

private const val CopyBufferBytes = 64 * 1024

/**
 * Streams `GET /documents/:id/export/<fmt>` into the cache and returns the file.
 *
 * It goes through the raw brokered client rather than the typed API because the
 * response is an octet stream whose length drives the progress bar, and because
 * the bytes must never be buffered whole: an export of a long document is
 * several megabytes.
 */
private suspend fun downloadExport(
    context: Context,
    clients: DocsClients,
    account: SharedAccount,
    document: DocumentDto,
    format: DocsExportFormat,
    progress: MutableStateFlow<Float?>,
): File = withContext(Dispatchers.IO) {
    val client = clients.raw(account)
    val request = Request.Builder()
        .url(documentExportUrl(account, document.id, format.id))
        .get()
        .build()

    client.okHttpClient.newCall(request).execute().use { response ->
        if (!response.isSuccessful) {
            val payload = runCatching { response.body.string() }.getOrNull()
            val parsed = payload?.let {
                runCatching { ExportJson.decodeFromString(ApiErrorDto.serializer(), it) }.getOrNull()
            }
            throw ExportHttpException(
                code = parsed?.error ?: DocErrorCode.INTERNAL,
                status = response.code,
            )
        }

        // Only the two declared cache folders are ever written to; anything else
        // would be unreachable to FileProvider and so unshareable.
        val dir = File(context.cacheDir, "downloads").apply { mkdirs() }
        val file = File(dir, exportFileName(document.title, format))
        val body = response.body
        val total = body.contentLength()
        body.byteStream().use { input ->
            file.outputStream().use { output ->
                val buffer = ByteArray(CopyBufferBytes)
                var copied = 0L
                while (true) {
                    val read = input.read(buffer)
                    if (read <= 0) break
                    output.write(buffer, 0, read)
                    copied += read
                    // A chunked response has no length: the bar goes indeterminate
                    // rather than showing a percentage it cannot compute.
                    progress.value = if (total > 0) (copied.toFloat() / total).coerceIn(0f, 1f) else null
                }
            }
        }
        file
    }
}

/**
 * Copies an exported file into the folder declared for outbound grants, so the
 * URI handed to another app points at a staging copy and never at the working
 * download (which a later export of the same document overwrites).
 */
private fun stageForShare(context: Context, file: File): File {
    val dir = File(context.cacheDir, "shared").apply { mkdirs() }
    val staged = File(dir, file.name)
    file.copyTo(staged, overwrite = true)
    return staged
}

/** Matches `${applicationId}.fileprovider` from the manifest. */
private fun fileProviderAuthority(context: Context): String = "${context.packageName}.fileprovider"

/** Strips what Android and the share targets choke on, and caps the length. */
private fun exportFileName(title: String, format: DocsExportFormat): String {
    val cleaned = title.trim().replace(Regex("[\\\\/:*?\"<>|]"), "_")
    return cleaned.ifEmpty { "document" }.take(120) + "." + format.extension
}

private fun formatLabel(format: DocsExportFormat): String = when (format) {
    DocsExportFormat.DOCX -> "Word (DOCX)"
    DocsExportFormat.ODT -> "OpenDocument (ODT)"
}

private fun formatHint(format: DocsExportFormat): String = when (format) {
    DocsExportFormat.DOCX -> "Format Microsoft Word"
    DocsExportFormat.ODT -> "Format ouvert OpenDocument"
}

/** The format the module would actually write back, as the web's label does. */
private fun saveTargetLabel(sourceFormat: String?): String =
    when (sourceFormat?.lowercase(Locale.ROOT)) {
        "odt" -> "ODT"
        // `.doc` is readable but not writable, and the module's own fallback for
        // a "save as" is DOCX — so that is what the row announces.
        else -> "DOCX"
    }

private fun humanSize(bytes: Long): String = when {
    bytes >= 1_048_576L -> String.format(Locale.getDefault(), "%.1f Mo", bytes / 1_048_576.0)
    bytes >= 1024L -> "${bytes / 1024} Ko"
    else -> "$bytes o"
}

/** A non-2xx on the raw export stream, carrying the module's error envelope. */
private class ExportHttpException(val code: String, val status: Int) : IOException("HTTP $status ($code)")

/** The module's error code, whichever transport reported the failure. */
private fun errorCodeOf(failure: Throwable): String? = when (failure) {
    is ExportHttpException -> failure.code
    is HttpException -> failure.response()?.errorBody()?.let { body ->
        runCatching { ExportJson.decodeFromString(ApiErrorDto.serializer(), body.string()) }
            .getOrNull()?.error
    }
    else -> null
}

private fun statusOf(failure: Throwable): Int? = when (failure) {
    is ExportHttpException -> failure.status
    is HttpException -> failure.code()
    else -> null
}

/**
 * True when save-to-source was refused because the origin has no writer. The
 * module raises this as `VALIDATION` (errors/mod.rs maps `OfficeError::Validation`
 * to 422 VALIDATION), while a converter blow-up is `CONVERSION_ERROR`; both are
 * 422 and both mean "do not retry, export instead".
 */
private fun isUnwritableOrigin(failure: Throwable): Boolean {
    if (statusOf(failure) != 422) return false
    val code = errorCodeOf(failure)
    return code == DocErrorCode.VALIDATION || code == DocErrorCode.CONVERSION_ERROR
}

/** Turns a failure into the line to show. Branches on the code, never the prose. */
private fun exportNoticeOf(failure: Throwable): ExportNotice {
    val text = when (errorCodeOf(failure)) {
        DocErrorCode.UNAUTHORIZED ->
            "Session expirée. Rouvrez le compte dans l'application qui l'a connecté, puis réessayez."
        DocErrorCode.FORBIDDEN, DocErrorCode.POLICY_DISABLED ->
            "Vous n'avez pas l'autorisation d'exporter ce document."
        DocErrorCode.NOT_FOUND ->
            "Ce document n'existe plus sur le serveur."
        DocErrorCode.CONVERSION_ERROR, DocErrorCode.VALIDATION ->
            "Ce document n'a pas pu être converti. Essayez l'autre format."
        null -> if (failure is IOException) {
            "Connexion impossible. Vérifiez votre réseau, puis réessayez."
        } else {
            "L'export a échoué. Réessayez."
        }
        else -> "L'export a échoué. Réessayez."
    }
    return ExportNotice(KubunoCalloutVariant.DANGER, text)
}
