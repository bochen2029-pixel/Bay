// One item row. Subscribes to its own item by id — by zustand's
// default strict-equality on the selected value, a change to any other
// item will not re-render this component. Drag handle + overflow menu
// are placeholders for I-06 / I-08.

import { useStore } from "../store";

export function Strip({ itemId }: { itemId: string }) {
  const item = useStore((s) => s.items[itemId]);
  if (!item) return null;

  return (
    <div className="strip" data-item-id={item.id}>
      <span className="strip-handle" aria-hidden="true">
        ≡
      </span>
      <span className="strip-content">{item.content}</span>
      <button
        className="strip-menu"
        type="button"
        aria-label="Item menu"
        disabled
      >
        ⋯
      </button>
    </div>
  );
}
