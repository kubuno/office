interface MathsLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Maths logo (designer artwork, raster). Served by the host from
 *  `/office-maths-logo.png`; rendered as a square image so it weighs the same
 *  as its neighbours in the waffle menu. */
export function MathsLogo({ size = 24, className, title = 'Maths' }: MathsLogoProps) {
  return (
    <img
      src="/office-maths-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default MathsLogo
