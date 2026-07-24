Event OnLoad()
    BlockActivation(!AllowContainerAccess, HideActivationTextWhenAccessBlocked)
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If GivingItems
        If FilterList != None
            Int i = 0
            Int listSize = FilterList.GetSize()
            While i < listSize
                Form filterItem = FilterList.GetAt(i)
                Int itemCount = GetItemCount(filterItem)
                If itemCount > 0
                    RemoveItem(filterItem, itemCount, false, akActionRef)
                EndIf
                i += 1
            EndWhile
        Else
            RemoveAllItems(akActionRef, false)
        EndIf
        If ActivateTextOverride != None
            ActivateTextOverride.Show()
        EndIf
    EndIf
EndEvent
