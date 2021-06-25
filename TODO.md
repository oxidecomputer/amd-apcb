* Maybe have iterators not have remaining_used_size in the first place, but just check buf len.  Is that possible?  We have to be able to use them to grow or shrink stuff.
* remaining_used_size == 0 and == sizeof are not the same thing!! Error out
* Add unit test for token entries!!  iteration, insertion, deletion, mutation
* Clean up .unwrap()
* Migrate tokens_entry.next_item
