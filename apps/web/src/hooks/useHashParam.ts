import { useEffect, useState } from "react";
import { hashParam } from "../lib/modules";

/** A query parameter of the current hash route, e.g. `note` in `#/notes?note=…`. */
export function useHashParam(name: string): string | undefined {
  const [value, setValue] = useState(() => hashParam(location.hash, name));
  useEffect(() => {
    const onHash = () => setValue(hashParam(location.hash, name));
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, [name]);
  return value;
}
