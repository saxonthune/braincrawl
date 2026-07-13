import { createSignal, For, type JSX } from "solid-js";

export interface DataTableColumn<T> {
  key: string;
  header: string;
  render: (row: T) => JSX.Element | string | number;
  sortValue?: (row: T) => string | number;
}

export interface DataTableProps<T> {
  columns: DataTableColumn<T>[];
  rows: T[];
}

export function DataTable<T>(props: DataTableProps<T>): JSX.Element {
  const [sortKey, setSortKey] = createSignal<string | null>(null);
  const [sortDir, setSortDir] = createSignal<1 | -1>(1);

  const sortedRows = () => {
    const key = sortKey();
    if (!key) return props.rows;
    const column = props.columns.find((c) => c.key === key);
    if (!column?.sortValue) return props.rows;
    const dir = sortDir();
    return [...props.rows].sort((a, b) => {
      const av = column.sortValue!(a);
      const bv = column.sortValue!(b);
      if (av < bv) return -1 * dir;
      if (av > bv) return 1 * dir;
      return 0;
    });
  };

  const toggleSort = (column: DataTableColumn<T>) => {
    if (!column.sortValue) return;
    if (sortKey() === column.key) {
      setSortDir((d) => (d === 1 ? -1 : 1));
    } else {
      setSortKey(column.key);
      setSortDir(1);
    }
  };

  return (
    <table class="data-table">
      <thead>
        <tr>
          <For each={props.columns}>
            {(column) => (
              <th classList={{ sortable: !!column.sortValue }} onClick={() => toggleSort(column)}>
                {column.header}
                {sortKey() === column.key ? (sortDir() === 1 ? " ▲" : " ▼") : ""}
              </th>
            )}
          </For>
        </tr>
      </thead>
      <tbody>
        <For each={sortedRows()}>
          {(row) => (
            <tr>
              <For each={props.columns}>{(column) => <td>{column.render(row)}</td>}</For>
            </tr>
          )}
        </For>
      </tbody>
    </table>
  );
}
