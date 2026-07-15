Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If !isEnabled
        If showActivatorDisabledMessage && disabledMessage != None
            disabledMessage.Show()
        EndIf
        Return
    EndIf

    isEnabled = False
    If delayItemGive > 0.0
        Utility.Wait(delayItemGive)
    EndIf
    GiveItems(akActionRef)

    If disableAfterGive
        Disable()
    EndIf
    If reActivateAfterTime > 0.0
        StartTimer(reActivateAfterTime, 1)
    ElseIf !disableAfterGive
        isEnabled = True
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        If disableAfterGive
            Enable()
        EndIf
        isEnabled = True
    EndIf
EndEvent

Function GiveItems(ObjectReference recipient)
    If itemsToGive == None
        Return
    EndIf

    Int itemIndex = 0
    While itemIndex < itemsToGive.Length
        If itemsToGive[itemIndex].itemToGive != None
            recipient.AddItem(itemsToGive[itemIndex].itemToGive, itemsToGive[itemIndex].count, !showItemAddMessage)
        EndIf
        itemIndex += 1
    EndWhile
EndFunction
