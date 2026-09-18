interface PresentationsLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Presentations logo (designer artwork, raster). Served by the host from
 *  `/office-presentations-logo.png`; rendered as a square image so it weighs the same
 *  as its neighbours in the waffle menu. */
export function PresentationsLogo({ size = 24, className, title = 'Presentations' }: PresentationsLogoProps) {
  return (
    <img
      src="/office-presentations-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default PresentationsLogo
