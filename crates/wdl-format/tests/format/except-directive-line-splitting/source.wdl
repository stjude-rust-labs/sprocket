version 1.2

# Test 1: Directive under length - should not split
#@ except: ShortRule
task test1 {}

# Test 2: Directive slightly over length - should split into 2 lines
#@ except: CommentWhitespace, DeprecatedObject, MetaDescription, InputSorted, ParameterMetaMatched
task test2 {}

# Test 3: Very long directive - should split into 3+ lines
#@ except: CommentWhitespace, DeprecatedObject, MetaDescription, InputSorted, ParameterMetaMatched, MatchingOutputMeta, RuntimeSectionKeys, RequirementsSectionKeys, HintsSectionKeys, OutputSectionKeys
task test3 {}

# Test 4: multiline - should be consolidated into one line
#@ except: FirstRule, SecondRule
#@ except: ThirdRule, FourthRule
task test4 {}

# Test 5: Exactly at max length - should not split (edge case)
#@ except: Rule1, Rule2, Rule3, Rule4, Rule5, Rule6, Rule7, Rule8
task test5 {}

# Test 6: Single very long rule name - must keep on one line (minimum 1 rule per line)
#@ except: ThisIsAnExtremelyLongRuleNameThatExceedsMaxLineLengthByItselfAndCannotBeSplitFurtherBecauseItsASingleRule
task test6 {}
