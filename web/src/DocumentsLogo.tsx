interface DocumentsLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Documents logo — the designer's artwork, served by the host as an image.
 *
 *  It used to be inlined as SVG paths. The artwork is now a bitmap, so drawing
 *  it inline would mean a base64 blob in the bundle, downloaded by everyone
 *  whether or not they ever open Documents. An `<img>` costs one cached request
 *  instead, and the file lives beside the other module logos the host serves. */
export function DocumentsLogo({ size = 24, className, title = 'Documents' }: DocumentsLogoProps) {
  return (
    <img
      src="/office-documents-logo.png"
      width={size}
      height={size}
      alt={title}
      title={title}
      className={className}
      // Kept square whatever the box it is placed in: the waffle menu and the
      // suite home give their icons different sizes.
      style={{ width: size, height: size, objectFit: 'contain', display: 'block' }}
      draggable={false}
    />
  )
}

export default DocumentsLogo
