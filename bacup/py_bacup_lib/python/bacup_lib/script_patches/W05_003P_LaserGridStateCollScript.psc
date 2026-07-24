Event OnLoad(ObjectReference akSenderRef)
    If OwningPlayer != None && OwningPlayer.GetReference() != None && UnlockItem != None && OwningPlayer.GetReference().GetItemCount(UnlockItem) > 0
        akSenderRef.DisableNoWait()
    EndIf
EndEvent
