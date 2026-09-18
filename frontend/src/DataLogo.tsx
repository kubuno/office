interface DataLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Data logo (designer artwork, raster). Served by the host from
 *  `/office-data-logo.png`; rendered as a square image so it weighs the same
 *  as its neighbours in the waffle menu. */
export function DataLogo({ size = 24, className, title = 'Data' }: DataLogoProps) {
  return (
    <img
      src="/office-data-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default DataLogo
