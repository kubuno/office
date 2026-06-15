interface OfficeLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Logo Office : grappe d'hexagones multicolores. Couleurs de marque fixes. */
export function OfficeLogo({ size = 24, className, title = 'Office' }: OfficeLogoProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 1180 1180"
      role="img"
      aria-label={title}
      className={className}
      style={{ fillRule: 'evenodd', clipRule: 'evenodd', strokeLinejoin: 'round', strokeMiterlimit: 2 }}
    >
      <title>{title}</title>
      <path d="M393.056,22.341l196.528,111.298l0,222.597l-196.528,111.298l-196.528,-111.298l-0,-222.597l196.528,-111.298Z" style={{ fill: '#5140f0' }} />
      <path d="M393.056,688.594l196.528,111.298l0,222.597l-196.528,111.298l-196.528,-111.298l-0,-222.597l196.528,-111.298Z" style={{ fill: '#5140f0' }} />
      <path d="M982.639,354.653l196.528,111.298l0,222.597l-196.528,111.298l-196.528,-111.298l-0,-222.597l196.528,-111.298Z" style={{ fill: '#f59e0b' }} />
      <path d="M786.111,688.594l196.528,111.298l0,222.597l-196.528,111.298l-196.528,-111.298l-0,-222.597l196.528,-111.298Z" style={{ fill: '#d51800' }} />
      <path d="M196.528,355.467l196.528,111.298l0,222.597l-196.528,111.298l-196.528,-111.298l-0,-222.597l196.528,-111.298Z" style={{ fill: '#16a34a' }} />
      <path d="M786.111,22.341l196.528,111.298l0,222.597l-196.528,111.298l-196.528,-111.298l-0,-222.597l196.528,-111.298Z" style={{ fill: '#2563eb' }} />
    </svg>
  )
}

export default OfficeLogo
