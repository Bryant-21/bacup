Scriptname B21_ActivateAliasWithRequiredItem Extends DefaultAliasOnActivate Const Default

Form Property ItemRequired Auto Const
Int Property NumItemsRequired = 1 Auto Const

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == None
        Return
    EndIf
    If ItemRequired != None && akActionRef.GetItemCount(ItemRequired) < NumItemsRequired
        Return
    EndIf
    Parent.OnActivate(akActionRef)
EndEvent
