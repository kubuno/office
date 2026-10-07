package com.kubuno.docs.ui.render

import android.util.Base64
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Category
import androidx.compose.material.icons.outlined.ImageNotSupported
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.isSpecified
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.isSpecified
import coil3.compose.AsyncImage
import coil3.compose.AsyncImagePainter
import coil3.request.ImageRequest
import coil3.request.crossfade
import com.kubuno.android.account.SharedAccount
import com.kubuno.android.ui.components.KubunoEmptyState
import com.kubuno.android.ui.components.KubunoEmptyTone
import com.kubuno.android.ui.components.KubunoSpinner
import com.kubuno.android.ui.components.KubunoSpinnerSize
import com.kubuno.docs.pm.PmAlign
import com.kubuno.docs.pm.PmNode
import com.kubuno.docs.ui.DocsPage
import com.kubuno.docs.ui.DocsShape
import com.kubuno.docs.ui.DocsType
import com.kubuno.docs.ui.docsPageInk
import com.kubuno.docs.ui.docsPageSurface
import com.kubuno.docs.ui.documentImageUrl
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import java.net.URLDecoder

/**
 * Rendering of a document's `image` node.
 *
 * Image bytes belong to the instance and are NEVER public: they are served by the
 * core proxy behind a bearer token. Every request therefore has to ride the
 * singleton [coil3.ImageLoader] built in `KubunoDocsApp`, whose fetcher uses
 * `DocsCallFactory` to pick the right borrowed account token by URL prefix.
 * Building an OkHttp client here — even "just for images" — would produce an
 * anonymous one and turn every inline image into a 401.
 */

/**
 * The account whose instance serves the document being rendered.
 *
 * A document body is rendered deep inside a block/inline tree, and threading the
 * account through every node would force each intermediate renderer to know
 * about accounts. A static local is the cheaper contract; the explicit `account`
 * parameter stays available for callers that already hold one.
 */
val LocalDocsAccount = staticCompositionLocalOf<SharedAccount?> { null }

/**
 * Renders a block image.
 *
 * The parameter order puts `account` before `modifier` on purpose: the common
 * call site in a block renderer is `PmImage(node, account)`, and the account is
 * the argument that changes the result, not the layout.
 *
 * @param availableWidth optional hard cap for the frame (a table cell, a quote).
 *   Unspecified means "whatever the parent offers".
 */
@Composable
fun PmImage(
    node: PmNode.Image,
    account: SharedAccount? = LocalDocsAccount.current,
    modifier: Modifier = Modifier,
    availableWidth: Dp = Dp.Unspecified,
) {
    // `align` is the node's own attribute; `wrap` describes how the desktop
    // canvas floats the image around the text. The phone reflows into a single
    // fluid column (see DocsPage), so a floating image is laid out as a block —
    // the alignment is kept, the wrapping is not reproducible without pages.
    val alignment = when (node.align) {
        PmAlign.CENTER -> Alignment.TopCenter
        PmAlign.RIGHT -> Alignment.TopEnd
        // JUSTIFY has no meaning for an atom: the web leaves it flush left too.
        PmAlign.LEFT, PmAlign.JUSTIFY -> Alignment.TopStart
    }

    val textBox = remember(node.alt) { textBoxContent(node.alt) }
    val declaredWidth: Dp? = node.width.takeIf { it > 0f }?.let { DocsPage.px(it) }
    val declaredHeight: Dp? = node.height.takeIf { it > 0f }?.let { DocsPage.px(it) }

    Box(modifier.fillMaxWidth(), contentAlignment = alignment) {
        when {
            // A text box is an image node whose real content lives in `alt`; its
            // `src` is only a blank frame the desktop canvas paints over. Showing
            // the frame would display an empty white rectangle, so the text wins.
            textBox != null -> TextBoxFrame(
                text = textBox,
                declaredWidth = declaredWidth,
                declaredHeight = declaredHeight,
                availableWidth = availableWidth,
            )

            else -> RemoteImage(
                node = node,
                account = account,
                declaredWidth = declaredWidth,
                declaredHeight = declaredHeight,
                availableWidth = availableWidth,
            )
        }
    }
}

/**
 * Convenience overload for a generic block: renders nothing for a node that is
 * not an image, so a block renderer can dispatch without a cast.
 */
@Composable
fun PmImage(
    node: PmNode,
    account: SharedAccount? = LocalDocsAccount.current,
    modifier: Modifier = Modifier,
) {
    if (node is PmNode.Image) PmImage(node, account, modifier)
}

@Composable
private fun RemoteImage(
    node: PmNode.Image,
    account: SharedAccount?,
    declaredWidth: Dp?,
    declaredHeight: Dp?,
    availableWidth: Dp,
) {
    // A data: URI is decoded here instead of being handed to the loader as a
    // string: the byte-array fetcher is part of the loader's default set, a
    // data-URI fetcher is not guaranteed to be.
    val model: Any? = remember(node.src, account?.serverUrl) {
        val raw = node.src.trim()
        when {
            raw.startsWith("data:") -> decodeDataUri(raw)
            account != null -> documentImageUrl(account, raw)
            // Without an account a relative src cannot be turned into an origin;
            // an absolute one is still fetchable, so keep it.
            raw.startsWith("http://") || raw.startsWith("https://") -> raw
            else -> null
        }
    }

    var failed by remember(model) { mutableStateOf(model == null) }
    var loaded by remember(model) { mutableStateOf(false) }
    var intrinsic by remember(model) { mutableStateOf(Size.Unspecified) }

    val density = LocalDensity.current
    val intrinsicWidth: Dp? = intrinsic.takeIf { it.isSpecified && it.width > 0f && it.width.isFinite() }
        ?.let { with(density) { it.width.toDp() } }
    val ratio: Float? = when {
        declaredWidth != null && declaredHeight != null && node.height > 0f -> node.width / node.height
        intrinsic.isSpecified && intrinsic.width > 0f && intrinsic.height > 0f -> intrinsic.width / intrinsic.height
        else -> null
    }

    // widthIn BEFORE fillMaxWidth so the frame settles on min(cap, parent width):
    // a 600 px illustration must shrink on a phone instead of overflowing the
    // reading column, and a 96 px icon must not be blown up to full width.
    // isSpecified, never `!= Dp.Unspecified`: the unspecified value is a NaN, so
    // equality on it is not reliable.
    val widthCap = listOfNotNull(availableWidth.takeIf { it.isSpecified }, declaredWidth ?: intrinsicWidth)
        .minOrNull()
    val frame = Modifier
        .then(if (widthCap != null) Modifier.widthIn(max = widthCap) else Modifier)
        .fillMaxWidth()
        .then(
            when {
                // aspectRatio keeps the shrunk frame proportional; a declared
                // height alone is only trusted until the real pixels arrive.
                ratio != null -> Modifier.aspectRatio(ratio)
                declaredHeight != null -> Modifier.height(declaredHeight)
                else -> Modifier.height(PlaceholderHeight)
            },
        )

    Box(frame, contentAlignment = Alignment.Center) {
        if (model != null) {
            AsyncImage(
                model = ImageRequest.Builder(LocalContext.current)
                    .data(model)
                    .crossfade(true)
                    .build(),
                contentDescription = node.altText ?: FallbackDescription,
                // Fit, not FillBounds: a declared box that disagrees with the
                // real pixels must not stretch the picture.
                contentScale = ContentScale.Fit,
                alignment = Alignment.Center,
                modifier = Modifier.fillMaxSize(),
                onState = { state ->
                    when (state) {
                        is AsyncImagePainter.State.Success -> {
                            loaded = true
                            failed = false
                            intrinsic = state.painter.intrinsicSize
                        }

                        is AsyncImagePainter.State.Error -> {
                            loaded = false
                            failed = true
                        }

                        is AsyncImagePainter.State.Loading -> {
                            loaded = false
                            failed = false
                        }

                        else -> Unit
                    }
                },
            )
        }
        when {
            failed -> ImageFallback(node)
            !loaded -> LoadingFrame()
        }
    }
}

/** The tinted ground + spinner shown while the bytes are on their way. */
@Composable
private fun LoadingFrame() {
    Box(
        Modifier
            .fillMaxSize()
            .clip(DocsShape.Chip)
            .background(MaterialTheme.colorScheme.surfaceVariant),
        contentAlignment = Alignment.Center,
    ) {
        KubunoSpinner(size = KubunoSpinnerSize.SM)
    }
}

/**
 * The failure placeholder: a sober framed area carrying the alternative text, so
 * the reader still knows what the author put there.
 */
@Composable
private fun ImageFallback(node: PmNode.Image) {
    val scheme = MaterialTheme.colorScheme
    val isShape = node.alt?.startsWith("kbshape:") == true
    val icon = if (isShape) Icons.Outlined.Category else Icons.Outlined.ImageNotSupported
    // A shape is a vector object the desktop regenerates from its `alt`; it is
    // not a broken image, so it gets its own wording rather than an error one.
    val title = if (isShape) ShapeTitle else UnavailableTitle

    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .clip(DocsShape.Chip)
            .background(scheme.surfaceVariant)
            .border(1.dp, scheme.outline, DocsShape.Chip),
        contentAlignment = Alignment.Center,
    ) {
        // The empty state needs room for its icon circle and two lines; a small
        // inline illustration only has room for the icon itself.
        if (maxWidth >= CompactFallbackWidth && maxHeight >= CompactFallbackHeight) {
            KubunoEmptyState(
                icon = icon,
                title = title,
                description = node.altText,
                tone = KubunoEmptyTone.UNAVAILABLE,
                compact = true,
            )
        } else {
            Icon(
                icon,
                contentDescription = node.altText ?: title,
                tint = scheme.onSurfaceVariant,
                modifier = Modifier.size(20.dp),
            )
        }
    }
}

/**
 * A text box (`kbtext:` / `kbtextrich:` payload on the `alt`). The desktop paints
 * the text onto the page canvas over a white frame with a grey hairline; the
 * phone reproduces the frame with the theme's own border so it stays legible in
 * dark mode, and reflows the text instead of clipping it to the stored height —
 * hence heightIn(min) rather than a fixed height.
 */
@Composable
private fun TextBoxFrame(
    text: String,
    declaredWidth: Dp?,
    declaredHeight: Dp?,
    availableWidth: Dp,
) {
    val widthCap = listOfNotNull(availableWidth.takeIf { it.isSpecified }, declaredWidth).minOrNull()
    Box(
        Modifier
            .then(if (widthCap != null) Modifier.widthIn(max = widthCap) else Modifier)
            .fillMaxWidth()
            .then(if (declaredHeight != null) Modifier.heightIn(min = declaredHeight) else Modifier)
            .clip(DocsShape.Chip)
            .background(docsPageSurface())
            .border(1.dp, MaterialTheme.colorScheme.outline, DocsShape.Chip)
            .padding(8.dp),
    ) {
        Text(text = text, style = DocsType.Body, color = docsPageInk())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

private val PlaceholderHeight = 160.dp
private val CompactFallbackWidth = 200.dp
private val CompactFallbackHeight = 120.dp

private const val FallbackDescription = "Image du document"
private const val UnavailableTitle = "Image indisponible"
private const val ShapeTitle = "Forme"

private val LenientJson = Json { ignoreUnknownKeys = true; isLenient = true }

/**
 * The text carried by a text-box `alt`, or null when the node is not one.
 *
 * Both payloads are `encodeURIComponent`-ed on the web. URLDecoder turns a bare
 * `+` into a space, which is harmless here: `encodeURIComponent` escapes `+` as
 * `%2B`, so a plus can never reach us unescaped.
 */
private fun textBoxContent(alt: String?): String? {
    val value = alt ?: return null
    return when {
        value.startsWith(RichTextBoxPrefix) ->
            decodeComponent(value.substring(RichTextBoxPrefix.length))?.let { richTextBoxText(it) }

        value.startsWith(TextBoxPrefix) ->
            decodeComponent(value.substring(TextBoxPrefix.length))?.takeIf { it.isNotBlank() }

        else -> null
    }
}

private const val TextBoxPrefix = "kbtext:"
private const val RichTextBoxPrefix = "kbtextrich:"

private fun decodeComponent(raw: String): String? =
    runCatching { URLDecoder.decode(raw, "UTF-8") }.getOrNull()

/** Flattens the ProseMirror document of a rich text box down to readable text. */
private fun richTextBoxText(json: String): String? = runCatching {
    val sb = StringBuilder()
    appendPmText(LenientJson.parseToJsonElement(json), sb, 0)
    sb.toString().trim().ifEmpty { null }
}.getOrNull()

private fun appendPmText(element: JsonElement, sb: StringBuilder, depth: Int) {
    // A payload comes straight from a document body; a hostile or corrupt one
    // must not be able to blow the stack.
    if (depth > MaxJsonDepth) return
    when (element) {
        is JsonObject -> {
            val type = (element["type"] as? JsonPrimitive)?.contentOrNull
            when (type) {
                "text" -> sb.append((element["text"] as? JsonPrimitive)?.contentOrNull.orEmpty())
                "hardBreak" -> sb.append('\n')
            }
            (element["content"] as? JsonArray)?.forEach { appendPmText(it, sb, depth + 1) }
            if (type == "paragraph" || type == "heading") sb.append('\n')
        }

        is JsonArray -> element.forEach { appendPmText(it, sb, depth + 1) }
        else -> Unit
    }
}

private const val MaxJsonDepth = 32

/** Decodes a `data:` URI into raw bytes, or null when it is not decodable here. */
private fun decodeDataUri(raw: String): ByteArray? {
    val comma = raw.indexOf(',')
    if (comma < 0) return null
    val meta = raw.substring(0, comma)
    val payload = raw.substring(comma + 1)
    return runCatching {
        if (meta.contains(";base64", ignoreCase = true)) {
            Base64.decode(payload, Base64.DEFAULT)
        } else {
            URLDecoder.decode(payload, "UTF-8").toByteArray()
        }
    }.getOrNull()
}
