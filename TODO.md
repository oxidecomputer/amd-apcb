* insert_entry: is that safe?!  How does it not check the new uninitialized entry?!
* insert_token into entry; i.e. resize entry; insert token at correct spot.
* remaining_used_size == 0 and == sizeof are not the same thing!! Error out
* Add unit test for token entries!!  iteration, insertion, deletion, mutation
