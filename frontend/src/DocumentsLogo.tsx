interface DocumentsLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Logo Documents : écusson bleu portant une page. Couleurs de marque fixes.
 *  Le viewBox est carré et recadré sur la bbox réelle du dessin (l'écusson est
 *  plus haut que large) pour que le logo pèse le même poids visuel que ses
 *  voisins dans le menu waffle. */
export function DocumentsLogo({ size = 24, className, title = 'Documents' }: DocumentsLogoProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="-3.334 0.839 60.602 60.602"
      role="img"
      aria-label={title}
      className={className}
      style={{ fillRule: 'evenodd', clipRule: 'evenodd', strokeLinecap: 'round', strokeLinejoin: 'round', strokeMiterlimit: 1.5 }}
    >
      <title>{title}</title>
      <path d="M24.257,1.565c1.677,-0.968 3.744,-0.968 5.421,0c5.169,2.984 16.378,9.456 21.546,12.44c1.677,0.968 2.711,2.758 2.711,4.695l0,24.88c-0,1.937 -1.033,3.726 -2.711,4.695c-5.169,2.984 -16.378,9.456 -21.546,12.44c-1.677,0.968 -3.744,0.968 -5.421,-0c-5.169,-2.984 -16.378,-9.456 -21.546,-12.44c-1.677,-0.968 -2.711,-2.758 -2.711,-4.695l-0,-24.88c0,-1.937 1.033,-3.726 2.711,-4.695c5.169,-2.984 16.378,-9.456 21.546,-12.44Z" style={{ fill: '#196fc7' }} />
      <path d="M31.334,14.002l9.492,9.809l0,23.161c0,0.719 -0.584,1.304 -1.304,1.304l-25.11,0c-0.719,0 -1.304,-0.584 -1.304,-1.304l0,-31.667c0,-0.719 0.584,-1.304 1.304,-1.304l16.921,-0Z" style={{ fill: 'none', stroke: '#fff', strokeWidth: '4px' }} />
      <path d="M31.631,23.809l9.195,0.002l-9.254,-9.329l0.059,9.327Z" style={{ fill: '#fff', stroke: '#fff', strokeWidth: '4px' }} />
      <path d="M20.261,32.072l12.963,-0" style={{ fill: 'none', stroke: '#fff', strokeWidth: '4px' }} />
      <path d="M20.261,39.853l12.963,0" style={{ fill: 'none', stroke: '#fff', strokeWidth: '4px' }} />
    </svg>
  )
}

export default DocumentsLogo
