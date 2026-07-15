Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || myQuest == None || myMessage == None
        Return
    EndIf

    If StagePreReq > 0 && !myQuest.IsStageDone(StagePreReq)
        Return
    EndIf
    If StageClose > 0 && myQuest.IsStageDone(StageClose)
        Return
    EndIf

    myMessage.Show()
EndEvent
