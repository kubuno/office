interface WhiteboardLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Whiteboard logo (designer artwork, raster). Served by the host from
 *  `/office-whiteboard-logo.png`; rendered as a square image so it weighs the same
 *  as its neighbours in the waffle menu. */
export function WhiteboardLogo({ size = 24, className, title = 'Whiteboard' }: WhiteboardLogoProps) {
  return (
    <img
      src="/office-whiteboard-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default WhiteboardLogo
