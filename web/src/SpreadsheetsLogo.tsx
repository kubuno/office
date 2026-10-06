interface SpreadsheetsLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Spreadsheets logo (designer artwork, raster). Served by the host from
 *  `/office-spreadsheets-logo.png`; rendered as a square image so it weighs the same
 *  as its neighbours in the waffle menu. */
export function SpreadsheetsLogo({ size = 24, className, title = 'Spreadsheets' }: SpreadsheetsLogoProps) {
  return (
    <img
      src="/office-spreadsheets-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default SpreadsheetsLogo
