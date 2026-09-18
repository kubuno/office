interface ProjectsLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Projects logo (designer artwork, raster). Served by the host from
 *  `/office-projects-logo.png`; rendered as a square image so it weighs the same
 *  as its neighbours in the waffle menu. */
export function ProjectsLogo({ size = 24, className, title = 'Projects' }: ProjectsLogoProps) {
  return (
    <img
      src="/office-projects-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default ProjectsLogo
