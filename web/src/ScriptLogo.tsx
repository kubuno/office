interface ScriptLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Script logo (designer artwork, raster). Served by the host from
 *  `/office-script-logo.png`; rendered as a square image so it weighs the same
 *  as its neighbours in the waffle menu. */
export function ScriptLogo({ size = 24, className, title = 'Script' }: ScriptLogoProps) {
  return (
    <img
      src="/office-script-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default ScriptLogo
