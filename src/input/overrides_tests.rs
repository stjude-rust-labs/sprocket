//! Unit tests for input overrides.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_handling() {
        // Test various error conditions
        
        // Empty key
        assert!(InputOverride::parse("=value").is_err());
        
        // Empty path component
        assert!(parse_path("workflow..param").is_err());
        
        // Empty array
        assert!(parse_flat_array("").is_err());
        
        // Trailing comma
        assert!(parse_flat_array("a,b,").is_err());
        
        // Leading comma
        assert!(parse_flat_array(",a,b").is_err());
        
        // Consecutive commas
        assert!(parse_flat_array("a,,b").is_err());
        
        // Unbalanced brackets
        assert!(parse_array_value("[a,b").is_err());
        assert!(parse_array_value("[a,b]]").is_err());
        
        // Empty element in array
        assert!(parse_array_value("[a,,b]").is_err());
        
        // Conflict validation
        let overrides = vec![
            InputOverride::parse("workflow=value").unwrap(),
            InputOverride::parse("workflow.param=value").unwrap(),
        ];
        assert!(validate_overrides(&overrides).is_err());
        
        let overrides = vec![
            InputOverride::parse("workflow.param=value").unwrap(),
            InputOverride::parse("workflow=value").unwrap(),
        ];
        assert!(validate_overrides(&overrides).is_err());
        
        let overrides = vec![
            InputOverride::parse("workflow.param=value").unwrap(),
            InputOverride::parse("workflow.param=other").unwrap(),
        ];
        assert!(validate_overrides(&overrides).is_err());
    }

    #[test]
    fn test_complex_nested_structures() {
        // Test deeply nested structures
        let result = parse_value("[[1,2],[3,4]],[[5,6]]").unwrap();
        
        match result {
            OverrideValue::Array(outer) => {
                assert_eq!(outer.len(), 2);
                
                match &outer[0] {
                    OverrideValue::Array(inner1) => {
                        assert_eq!(inner1.len(), 2);
                        
                        match &inner1[0] {
                            OverrideValue::Array(inner2) => {
                                assert_eq!(inner2.len(), 2);
                                assert!(matches!(inner2[0], OverrideValue::Integer(1)));
                                assert!(matches!(inner2[1], OverrideValue::Integer(2)));
                            },
                            _ => panic!("Expected array"),
                        }
                        
                        match &inner1[1] {
                            OverrideValue::Array(inner2) => {
                                assert_eq!(inner2.len(), 2);
                                assert!(matches!(inner2[0], OverrideValue::Integer(3)));
                                assert!(matches!(inner2[1], OverrideValue::Integer(4)));
                            },
                            _ => panic!("Expected array"),
                        }
                    },
                    _ => panic!("Expected array"),
                }
                
                match &outer[1] {
                    OverrideValue::Array(inner1) => {
                        assert_eq!(inner1.len(), 1);
                        
                        match &inner1[0] {
                            OverrideValue::Array(inner2) => {
                                assert_eq!(inner2.len(), 2);
                                assert!(matches!(inner2[0], OverrideValue::Integer(5)));
                                assert!(matches!(inner2[1], OverrideValue::Integer(6)));
                            },
                            _ => panic!("Expected array"),
                        }
                    },
                    _ => panic!("Expected array"),
                }
            },
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_empty_arrays() {
        // Test empty arrays
        let result = parse_array_value("[]").unwrap();
        match result {
            OverrideValue::Array(values) => {
                assert_eq!(values.len(), 0);
            },
            _ => panic!("Expected array"),
        }
        
        let result = parse_array_value("[[],[]]").unwrap();
        match result {
            OverrideValue::Array(outer) => {
                assert_eq!(outer.len(), 2);
                assert!(matches!(&outer[0], OverrideValue::Array(inner) if inner.is_empty()));
                assert!(matches!(&outer[1], OverrideValue::Array(inner) if inner.is_empty()));
            },
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_mixed_types_in_arrays() {
        // Test arrays with mixed types
        let result = parse_value("[1,true,null,3.14,hello]").unwrap();
        
        match result {
            OverrideValue::Array(values) => {
                assert_eq!(values.len(), 5);
                assert!(matches!(values[0], OverrideValue::Integer(1)));
                assert!(matches!(values[1], OverrideValue::Boolean(true)));
                assert!(matches!(values[2], OverrideValue::Null));
                assert!(matches!(values[3], OverrideValue::Float(3.14)));
                assert!(matches!(values[4], OverrideValue::String(ref s) if s == "hello"));
            },
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_apply_to_existing_structure() {
        // Test applying overrides to existing nested structure
        let base_json: Value = serde_json::from_str(r#"
            {
                "workflow": {
                    "task": {
                        "param1": "old",
                        "param2": 42,
                        "array": [1, 2, 3],
                        "nested": {
                            "deep": "value"
                        }
                    }
                }
            }
        "#).unwrap();
        
        let overrides = vec![
            InputOverride::parse("workflow.task.param1=new").unwrap(),
            InputOverride::parse("workflow.task.param3=added").unwrap(),
            InputOverride::parse("workflow.task.array=[4,5,6]").unwrap(),
            InputOverride::parse("workflow.task.nested.deep=updated").unwrap(),
            InputOverride::parse("workflow.task.nested.deeper=created").unwrap(),
        ];
        
        let result = apply_overrides(base_json, &overrides).unwrap();
        
        // Check direct value updates
        assert_eq!(result["workflow"]["task"]["param1"], "new");
        assert_eq!(result["workflow"]["task"]["param2"], 42); // unchanged
        assert_eq!(result["workflow"]["task"]["param3"], "added");
        
        // Check array replacement
        let array = result["workflow"]["task"]["array"].as_array().unwrap();
        assert_eq!(array.len(), 3);
        assert_eq!(array[0], 4);
        assert_eq!(array[1], 5);
        assert_eq!(array[2], 6);
        
        // Check nested updates
        assert_eq!(result["workflow"]["task"]["nested"]["deep"], "updated");
        assert_eq!(result["workflow"]["task"]["nested"]["deeper"], "created");
    }

    #[test]
    fn test_null_handling() {
        // Test handling of null values
        let base_json: Value = serde_json::from_str(r#"
            {
                "workflow": {
                    "param": "value",
                    "nullable": null
                }
            }
        "#).unwrap();
        
        let overrides = vec![
            InputOverride::parse("workflow.param=null").unwrap(),
            InputOverride::parse("workflow.nullable.nested=created").unwrap(),
        ];
        
        let result = apply_overrides(base_json, &overrides).unwrap();
        
        // Check null replacement
        assert!(result["workflow"]["param"].is_null());
        
        // Check creating through null
        assert_eq!(result["workflow"]["nullable"]["nested"], "created");
    }

    #[test]
    fn test_wdl_specific_examples() {
        // Test examples from the PR doc
        
        // Primitives
        assert!(matches!(parse_value("Alice").unwrap(), OverrideValue::String(s) if s == "Alice"));
        assert!(matches!(parse_value("\"Alice\"").unwrap(), OverrideValue::String(s) if s == "Alice"));
        assert!(matches!(parse_value("200").unwrap(), OverrideValue::Integer(200)));
        assert!(matches!(parse_value("3.14").unwrap(), OverrideValue::Float(3.14)));
        assert!(matches!(parse_value("true").unwrap(), OverrideValue::Boolean(true)));
        assert!(matches!(parse_value("/path/to/file").unwrap(), OverrideValue::String(s) if s == "/path/to/file"));
        
        // Arrays
        let tags = parse_value("dev,test").unwrap();
        match tags {
            OverrideValue::Array(values) => {
                assert_eq!(values.len(), 2);
                assert!(matches!(&values[0], OverrideValue::String(s) if s == "dev"));
                assert!(matches!(&values[1], OverrideValue::String(s) if s == "test"));
            },
            _ => panic!("Expected array"),
        }
        
        // Nested arrays
        let nested = parse_value("[dev,test],[prod]").unwrap();
        match nested {
            OverrideValue::Array(outer) => {
                assert_eq!(outer.len(), 2);
                
                match &outer[0] {
                    OverrideValue::Array(inner) => {
                        assert_eq!(inner.len(), 2);
                        assert!(matches!(&inner[0], OverrideValue::String(s) if s == "dev"));
                        assert!(matches!(&inner[1], OverrideValue::String(s) if s == "test"));
                    },
                    _ => panic!("Expected array"),
                }
                
                match &outer[1] {
                    OverrideValue::Array(inner) => {
                        assert_eq!(inner.len(), 1);
                        assert!(matches!(&inner[0], OverrideValue::String(s) if s == "prod"));
                    },
                    _ => panic!("Expected array"),
                }
            },
            _ => panic!("Expected array"),
        }
        
        // Complex example from PR doc
        let base_json: Value = serde_json::from_str(r#"
            {
                "read_group": {"ID": "rg1", "PI": 150, "PL": "ILLUMINA"},
                "complex_map": {"batch1": [[[["1", "old"]]]]}
            }
        "#).unwrap();
        
        let overrides = vec![
            InputOverride::parse("read_group.ID=rg2").unwrap(),
            InputOverride::parse("complex_map.batch1=[[1,a],[2,b]],[[3,c]]").unwrap(),
            InputOverride::parse("complex_map.batch2=[[4,d],[5,e]],[[6,f]],[7,g]").unwrap(),
        ];
        
        let result = apply_overrides(base_json, &overrides).unwrap();
        
        // Check read_group updates
        assert_eq!(result["read_group"]["ID"], "rg2");
        assert_eq!(result["read_group"]["PI"], 150); // unchanged
        assert_eq!(result["read_group"]["PL"], "ILLUMINA"); // unchanged
        
        // Check complex_map.batch1
        let batch1 = &result["complex_map"]["batch1"];
        assert!(batch1.is_array());
        
        // Check complex_map.batch2
        let batch2 = &result["complex_map"]["batch2"];
        assert!(batch2.is_array());
    }
} 