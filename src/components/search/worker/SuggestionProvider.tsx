import {
  createContext,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode
} from "react";
import { SuggestionClient } from "./SuggestionClient";
import type { ActiveToken, TagSuggestion } from "../types";
const Context = createContext<SuggestionClient | null>(null);
export function SuggestionProvider({ children }: { children: ReactNode }) {
  const [client] = useState(() => new SuggestionClient());
  useEffect(() => () => client.dispose(), [client]);
  return <Context.Provider value={client}>{children}</Context.Provider>;
}
const EMPTY: TagSuggestion[] = [];
export function useSuggestions(
  tags: string[],
  token: ActiveToken | null,
  used: Set<string>,
  eligible: boolean,
  excludedTags: string[]
) {
  const shared = useContext(Context);
  const [local] = useState(() => (shared ? null : new SuggestionClient()));
  const client = shared ?? local!;
  const editor = useId();
  const [items, setItems] = useState<TagSuggestion[]>(EMPTY);
  const [failed, setFailed] = useState(false);
  const [retry, setRetry] = useState(0);
  const latest = useRef({ token, used, excludedTags });
  latest.current = { token, used, excludedTags };
  const key = JSON.stringify([eligible, token, [...used], excludedTags, retry]);
  const [resultKey, setResultKey] = useState("");
  useEffect(() => {
    let current = true;
    const { token, used, excludedTags } = latest.current;
    if (!eligible || !token || !token.query.trim() || (!token.literal && !token.negative && /^(gn|tags)(:.*)?$/i.test(token.query)))
      return;
    setFailed(false);
    void client
      .search(editor, tags, token, [...used], excludedTags)
      .then((next) => {
        if (current) {
          setItems(next);
          setResultKey(key);
        }
      })
      .catch(() => {
        if (current) setFailed(true);
      });
    return () => {
      current = false;
      client.cancel(editor);
    };
  }, [client, editor, tags, key, eligible]);
  useEffect(() => () => local?.dispose(), [local]);
  return {
    suggestions: resultKey === key && eligible ? items : EMPTY,
    failed,
    retry: () => {
      client.retry();
      setRetry((value) => value + 1);
    }
  };
}
