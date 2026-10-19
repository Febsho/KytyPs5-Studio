type Item = { id: string; label: string };
type Props = { items: Item[]; active: string; onNavigate: (id: string) => void };

const labelFor = (label: string) => label === 'Home' ? 'Games' : label === 'Fork Manager' ? 'Repositories' : label === 'Diagnostics' ? 'System Status' : label === 'Community Builds' ? 'Community PR Builds' : label;

export function MainNavigation({ items, active, onNavigate }: Props) {
  const overflow = items.slice(4);
  return <nav className="main-nav" aria-label="Main navigation">
    <div className="main-nav-links">{items.map(item => <button className={active === item.id ? 'active' : ''} key={item.id} onClick={() => onNavigate(item.id)}>{labelFor(item.label)}</button>)}</div>
    {overflow.length > 0 && <details className={`nav-more ${overflow.some(item => item.id === active) ? 'active' : ''}`}>
      <summary tabIndex={0}>More</summary>
      <div className="nav-more-menu" aria-label="More sections">{overflow.map(item => <button key={item.id} aria-current={active === item.id ? 'page' : undefined} onClick={event => { onNavigate(item.id); event.currentTarget.closest('details')?.removeAttribute('open'); }}>{labelFor(item.label)}</button>)}</div>
    </details>}
  </nav>;
}
