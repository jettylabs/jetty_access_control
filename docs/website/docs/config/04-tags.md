# Tags

Jetty lets users define and manage tags to make it easier to track access to sensitive or important information. These tags are managed via a configuration file that can be found in your project directory under `tags/tags.yaml`.

The `tags.yaml` file contains a dictionary of tag names and tag configurations, that could look something like this:

```yaml title="tags/tags.yaml"
Customer PII:
    description: Contains customer PII, including name, address, and phone number
    pass_through_lineage: true
    pass_through_hierarchy: false
    apply_to:
        - warehouse/raw/customer
    remove_from:
        - warehouse/sanitized/masked_customer
```

### Answering Questions

By setting up tags, you can easily answer questions like:

-   Who has access to data that may contain PII or financial data?
-   What dashboards or metrics may contain sensitive information?

## Tag Configurations

Tags are applied to data assets based on the following configurations:

-   **description** (optional) - A string describing the tag. This makes it easier to search for and identify specific tags
-   **pass_through_lineage** (optional, default: `false`) - Whether this tag should be inherited by assets derived from the those the tag is explicitly applied to
-   **pass_through_hierarchy** (optional, default: `false`) - Whether this tag should be inherited by assets that descend in hierarchy from those the tag is explicitly applied to (for example, tables could inherit tags applied to their schema)
-   **apply_to** (required) - A list of assets that this tag should be directly applied to. Jetty will try to infer which asset you are referring to based on the text you provide (for example, you could use `customer_table` to refer to `snowflake::ANALYTICS_DB/CUSTOMER_SCHEMA/ALL_CUSTOMER_TABLE`). If the text provided could reference multiple assets and then try to run a Jetty command, the system will show you all the potential matches so that you can update tags.yaml with something more specific. When necessary, an asset reference can include a name and type value (both of these will be provided in the error message in the case of an ambiguous reference). These detailed references can be used alongside simple string references. This would look like the following:

```yaml
Customer PII:
    apply_to:
        - name: customer
          type: workbook
        - warehouse/raw/customer
```

-   **remove_from** (optional) - A list of assets that this tag should be removed from. This is useful for tags that are passed through lineage or hierarchy, but should now longer apply after a certain point (if sensitive data has been masked, for example). Asset matching works the same way as it does for the `apply_to` field.
