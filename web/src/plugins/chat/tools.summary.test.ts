import { describe, expect, it } from "vite-plus/test";
import { summarizeWork, summarizeWorks } from "./tools";

const RECORD = {
  id: "https://openalex.org/W123",
  display_name: "Display Title",
  title: "Fallback Title",
  publication_year: 2020,
  authorships: [
    { author: { display_name: "Ada Lovelace" } },
    { author: { display_name: "Alan Turing" } },
  ],
};

describe("summarizeWork", () => {
  it("shapes the record into { id, title, year, authors }", () => {
    expect(summarizeWork(RECORD)).toEqual({
      id: "https://openalex.org/W123",
      title: "Display Title",
      year: 2020,
      authors: ["Ada Lovelace", "Alan Turing"],
    });
  });

  it("falls back to title when display_name is missing", () => {
    const { display_name, ...rest } = RECORD;
    expect(summarizeWork(rest).title).toBe("Fallback Title");
  });
});

describe("summarizeWorks", () => {
  it("maps summarizeWork over every record", () => {
    expect(summarizeWorks([RECORD, RECORD])).toEqual([
      summarizeWork(RECORD),
      summarizeWork(RECORD),
    ]);
  });
});
