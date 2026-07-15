import { authPlugin } from "./auth";
import { chat } from "./chat";
import { documentIndex } from "./document-index";
import { readingList } from "./reading-list";
import type { Plugin } from "./types";

export const plugins: Plugin[] = [documentIndex, readingList, chat, authPlugin];
