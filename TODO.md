* insert_token into entry; i.e. resize entry; insert token at correct spot.
* remaining_used_size == 0 and == sizeof are not the same thing!! Error out
* Add unit test for token entries!!  iteration, insertion, deletion, mutation
* Clean up .unwrap()
* GroupMutItem and GroupMutIter are currently the same thing--maybe split them up, too.
* review resize_group_by error paths
