import { useEffect, useMemo, useRef } from "react";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  pointerWithin,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import { CSS } from "@dnd-kit/utilities";
import { useTranslation } from "react-i18next";
import { cn } from "../utils";
import {
  BOARD_LANES,
  groupCardsByLane,
  laneForTargets,
  type BoardCardModel,
  type BoardLane,
} from "./boardLanes";

/** Static class strings so Tailwind keeps the lane cue colors in the build. */
const LANE_STYLE: Record<BoardLane, { dot: string; text: string; tint: string }> = {
  library: { dot: "bg-lane-library", text: "text-lane-library", tint: "bg-lane-library-bg" },
  codex: { dot: "bg-lane-codex", text: "text-lane-codex", tint: "bg-lane-codex-bg" },
  claude: { dot: "bg-lane-claude", text: "text-lane-claude", tint: "bg-lane-claude-bg" },
  both: { dot: "bg-lane-both", text: "text-lane-both", tint: "bg-lane-both-bg" },
};

interface ArtifactBoardProps {
  cards: BoardCardModel[];
  selectedId: string | null;
  onSelect: (card: BoardCardModel) => void;
  /** Called only when the drop lane differs from the card's current lane. */
  onMoveToLane: (card: BoardCardModel, lane: BoardLane) => void;
  /** Context label for false/false. Central Library keeps the default Library label. */
  libraryLaneLabel?: string;
  /** Cards with an in-flight target mutation. */
  pendingIds?: Set<string>;
}

export function ArtifactBoard({
  cards,
  selectedId,
  onSelect,
  onMoveToLane,
  libraryLaneLabel,
  pendingIds,
}: ArtifactBoardProps) {
  const { t } = useTranslation();
  const boardScrollRef = useRef<HTMLDivElement>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    // Space starts a keyboard drag; Enter stays free to open the Inspector and
    // Escape stays free to close it.
    useSensor(KeyboardSensor, {
      keyboardCodes: { start: ["Space"], cancel: ["Escape"], end: ["Space"] },
    }),
  );

  const grouped = useMemo(() => groupCardsByLane(cards), [cards]);
  const selectedLane = useMemo(() => {
    const selectedCard = cards.find((card) => card.id === selectedId);
    return selectedCard ? laneForTargets(selectedCard.canonicalTargets) : null;
  }, [cards, selectedId]);

  useEffect(() => {
    if (!selectedLane) return;
    const scroller = boardScrollRef.current;
    if (!scroller) return;

    const scrollSelectedLaneIntoView = () => {
      const lane = scroller.querySelector<HTMLElement>(`[data-board-lane="${selectedLane}"]`);
      if (!lane) return;

      const scrollerBounds = scroller.getBoundingClientRect();
      const laneBounds = lane.getBoundingClientRect();
      const leftOverflow = laneBounds.left - scrollerBounds.left;
      const rightOverflow = laneBounds.right - scrollerBounds.right;
      if (leftOverflow >= 0 && rightOverflow <= 0) return;

      scroller.scrollTo({
        left: Math.max(
          0,
          scroller.scrollLeft + (leftOverflow < 0 ? leftOverflow : rightOverflow),
        ),
        behavior: "auto",
      });
    };

    let animationFrame = requestAnimationFrame(scrollSelectedLaneIntoView);
    const resizeObserver = new ResizeObserver(() => {
      cancelAnimationFrame(animationFrame);
      animationFrame = requestAnimationFrame(scrollSelectedLaneIntoView);
    });
    resizeObserver.observe(scroller);

    return () => {
      cancelAnimationFrame(animationFrame);
      resizeObserver.disconnect();
    };
  }, [selectedLane]);

  const handleDragEnd = (event: DragEndEvent) => {
    const lane = event.over?.id as BoardLane | undefined;
    if (!lane) return;
    const card = cards.find((item) => item.id === event.active.id);
    if (!card) return;
    // Dropping back into the source lane is not a target change.
    if (laneForTargets(card.canonicalTargets) === lane) return;
    onMoveToLane(card, lane);
  };

  return (
    <DndContext sensors={sensors} collisionDetection={pointerWithin} onDragEnd={handleDragEnd}>
      <div
        ref={boardScrollRef}
        className="flex min-h-0 flex-1 gap-4 overflow-x-auto overflow-y-hidden pb-4"
      >
        {BOARD_LANES.map((lane) => (
          <BoardLaneColumn
            key={lane}
            lane={lane}
            label={lane === "library" ? libraryLaneLabel ?? t("board.lanes.library") : t(`board.lanes.${lane}`)}
            emptyLabel={t("board.laneEmpty")}
            cards={grouped[lane]}
            selectedId={selectedId}
            onSelect={onSelect}
            pendingIds={pendingIds}
          />
        ))}
      </div>
    </DndContext>
  );
}

interface BoardLaneColumnProps {
  lane: BoardLane;
  label: string;
  emptyLabel: string;
  cards: BoardCardModel[];
  selectedId: string | null;
  onSelect: (card: BoardCardModel) => void;
  pendingIds?: Set<string>;
}

function BoardLaneColumn({
  lane,
  label,
  emptyLabel,
  cards,
  selectedId,
  onSelect,
  pendingIds,
}: BoardLaneColumnProps) {
  const { isOver, setNodeRef } = useDroppable({ id: lane });
  const style = LANE_STYLE[lane];

  return (
    <section
      ref={setNodeRef}
      data-board-lane={lane}
      className={cn(
        "flex h-full min-h-0 w-[280px] shrink-0 flex-col overflow-hidden rounded-xl border border-border-subtle bg-bg-secondary transition-colors",
        isOver && "border-action bg-action-bg",
      )}
    >
      <header className="flex shrink-0 items-center gap-2 border-b border-border-subtle px-3 py-2.5">
        <span className={cn("h-2 w-2 shrink-0 rounded-full", style.dot)} />
        <h2 className={cn("truncate text-[13px] font-semibold", style.text)}>{label}</h2>
        <span className="ml-auto rounded-full bg-surface-hover px-2 text-[12px] font-medium leading-[18px] tabular-nums text-muted">
          {cards.length}
        </span>
      </header>
      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-2 scrollbar-hide">
        {cards.length === 0 ? (
          <p className="px-2 py-6 text-center text-[12px] text-faint">{emptyLabel}</p>
        ) : (
          cards.map((card) => (
            <BoardCard
              key={card.id}
              card={card}
              lane={lane}
              selected={card.id === selectedId}
              pending={pendingIds?.has(card.id) ?? false}
              onSelect={onSelect}
            />
          ))
        )}
      </div>
    </section>
  );
}

interface BoardCardProps {
  card: BoardCardModel;
  lane: BoardLane;
  selected: boolean;
  pending: boolean;
  onSelect: (card: BoardCardModel) => void;
}

function BoardCard({ card, lane, selected, pending, onSelect }: BoardCardProps) {
  const { t } = useTranslation();
  const { attributes, listeners, setNodeRef, transform, isDragging } = useDraggable({
    id: card.id,
    disabled: pending,
  });
  const style = transform
    ? { transform: CSS.Translate.toString(transform), zIndex: 30 }
    : undefined;

  return (
    <article
      ref={setNodeRef}
      style={style}
      {...attributes}
      {...listeners}
      onClick={() => onSelect(card)}
      onKeyDown={(event) => {
        if (event.key !== "Enter") return;
        event.preventDefault();
        onSelect(card);
      }}
      // aria-pressed is owned by dnd-kit's drag state, so selection uses aria-current.
      aria-current={selected ? "true" : undefined}
      className={cn(
        "cursor-grab rounded-lg border bg-surface p-3 text-left shadow-card transition-colors active:cursor-grabbing",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-focus-ring)]",
        selected ? "border-action ring-1 ring-action" : "border-border-subtle hover:bg-surface-hover",
        isDragging && "opacity-50",
        pending && "cursor-wait opacity-60",
      )}
    >
      <div className="flex items-start gap-2">
        <h3 className="min-w-0 flex-1 truncate text-[13px] font-semibold text-primary" title={card.title}>
          {card.title}
        </h3>
        <span className={cn("shrink-0 rounded px-1.5 text-[11px] leading-5", LANE_STYLE[lane].tint, LANE_STYLE[lane].text)}>
          {card.artifactType}
        </span>
      </div>
      <p
        className={cn(
          "mt-1.5 line-clamp-2 text-[12px] leading-5",
          card.summary ? "text-muted" : "text-faint",
        )}
      >
        {card.summary ?? t("board.unavailable")}
      </p>
      {(card.version || card.status || card.otherTargets.length > 0) && (
        <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[11px] text-faint">
          {card.version && <span className="tabular-nums">{card.version}</span>}
          {card.status && <span>{card.status}</span>}
          {card.otherTargets.length > 0 && (
            <span title={card.otherTargets.join(", ")}>
              {t("board.otherTargets", { count: card.otherTargets.length })}
            </span>
          )}
        </div>
      )}
    </article>
  );
}
