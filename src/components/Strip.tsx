// One item row. Subscribes to its own item by id — by zustand's
// default strict-equality on the selected value, a change to any other
// item will not re-render this component.
//
// I-06 wraps the content in @dnd-kit/sortable's `useSortable` so the
// strip is draggable within its bay. The grip (::handle) owns the
// drag listener; the content area stays clickable for future
// inspector-open behavior.

import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import { useStore } from "../store";

export function Strip({ itemId }: { itemId: string }) {
  const item = useStore((s) => s.items[itemId]);

  // Hooks must run unconditionally; ask for sortable state regardless
  // of whether the item is present, and early-return below.
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: itemId });

  if (!item) return null;

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.4 : 1,
  } as const;

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={"strip" + (isDragging ? " is-dragging" : "")}
      data-item-id={item.id}
      {...attributes}
    >
      <span
        className="strip-handle"
        aria-label="Drag handle"
        {...listeners}
      >
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
