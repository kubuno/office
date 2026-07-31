import { openImagePicker } from '@kubuno/sdk'

/**
 * Thin wrappers around the core image picker — THE single entry point for every
 * "insert an image" action in office. Modules never build their own file input.
 */

/** Reads a local file as a data URL: how office embeds images so a document stays self-contained. */
function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload  = () => resolve(String(reader.result))
    reader.onerror = () => reject(new Error('file read failed'))
    reader.readAsDataURL(file)
  })
}

/**
 * Opens the picker and resolves with a `src` usable as-is: the chosen URL kept as
 * a reference, or a data URL when the user supplied a local file.
 *
 * Returns null if the user closes the picker.
 */
export async function pickImageSrc(title?: string): Promise<string | null> {
  const picked = await openImagePicker(title ? { title } : undefined)
  if (!picked) return null
  return picked.kind === 'url' ? picked.url : await fileToDataUrl(picked.file)
}

/**
 * Same picker, but always resolves to a File — for the callers that UPLOAD the
 * image rather than merely reference it. A picked URL is fetched here, because an
 * authenticated Drive/Photos URL is useless to whoever opens the document later.
 *
 * Returns null if the user closes the picker; throws if a picked URL is unreachable.
 *
 * NOTE: mirrors `pickImageFile` from the core, which is not part of the published
 * @kubuno/sdk type surface yet — drop this in favour of the SDK export once it is.
 */
