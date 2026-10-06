interface DiagramsLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Diagrams logo (designer artwork, raster). Served by the host from
 *  `/office-diagrams-logo.png`; rendered as a square image so it weighs the same
 *  as its neighbours in the waffle menu. */
export function DiagramsLogo({ size = 24, className, title = 'Diagrams' }: DiagramsLogoProps) {
  return (
    <img
      src="/office-diagrams-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default DiagramsLogo
