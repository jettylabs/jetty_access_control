<template>
  <div class="q-pa-md">
    <q-table
      :title="props.title"
      :rows="rows"
      :columns="props.columns"
      row-key="name"
      flat
      :rows-per-page-options="[props.rowsPerPage]"
      :pagination="pagination"
      wrap-cells
      :filter="tableFilter"
      :filter-method="props.filterMethod"
      ref="jettyTable"
      dense
    >
      <template v-slot:top>
        <div class="flex justify-between fit">
          <div class="q-table__title">
            {{ props.title }}
            <q-badge v-if="props.tip" outline class="q-px-none" align="top">
              <q-icon name="help_outline" color="secondary" size="1rem">
                <q-tooltip class="text-body2" max-width="300px">
                  {{ props.tip }}
                </q-tooltip>
              </q-icon>
            </q-badge>
          </div>
          <div class="flex">
            <q-input outlined dense v-model="tableFilter">
              <template v-slot:prepend>
                <q-icon name="o_filter_alt" />
              </template>
            </q-input>
            <q-btn
              color="primary"
              icon-right="download"
              no-caps
              @click="exportTable"
              outline
              class="q-ml-xs q-px-sm"
            />
          </div>
        </div>
      </template>

      <template v-slot:body="props">
        <slot :props="props"> </slot>
      </template>
    </q-table>
  </div>
</template>

<script setup>
import { ref, reactive } from "vue";
import { downloadCSV, fetchJson } from "src/util";

const props = defineProps([
  "title",
  "rowsPerPage",
  "filterMethod",
  "columns",
  "csvConfig",
  "fetchPath",
  "tip",
]);

var rows = ref([]);

const pagination = ref({
  sortBy: "name",
  descending: false,
  page: 1,
  rowsPerPage: props.rowsPerPage,
});

const tableFilter = ref(null);

const jettyTable = ref(null);
const exportTable = () => {
  downloadCSV(
    props.csvConfig.filename,
    props.csvConfig.columnNames,
    props.csvConfig.mappingFn(jettyTable.value.filteredSortedRows)
  );
};

fetchJson(props.fetchPath)
  .then((r) => (rows.value = r))
  .catch((error) => console.log("unable to fetch: ", error));
</script>