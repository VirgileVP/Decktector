import { PopOutButton } from './PopOutButton'
import { ShowEditor } from './ShowEditor'

/**
 * The show editor's frame: inline in the Settings "Show" tab, or the whole
 * of a Show panel window. Popping it out opens the same editor in its own
 * window, sized for the table.
 */
export function ShowPanel(): React.JSX.Element {
  return (
    <section className="show-panel poppable" aria-label="Show">
      <PopOutButton panelId="show" deckId={null} title="Open the show in its own window" />
      <ShowEditor />
    </section>
  )
}
