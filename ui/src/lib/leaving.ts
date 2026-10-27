// Keeps something on screen a little after it is gone, so it can animate out.
import { useEffect, useState } from "react";

/** The last non-null `value`, kept for `ms` after `value` goes back to null; `leaving` is true
 * meanwhile. */
export function useLeaving<T>(value: T | null, ms: number): { shown: T | null; leaving: boolean } {
  const [shown, setShown] = useState(value);
  if (value !== null && value !== shown) setShown(value);
  useEffect(() => {
    if (value !== null) return;
    const timer = setTimeout(() => setShown(null), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return { shown, leaving: value === null && shown !== null };
}
