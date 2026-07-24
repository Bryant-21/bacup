Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If linkedGame == None
        Return
    EndIf
    If linkedGame.myPlayer == akActionRef
        If AlreadyRegisteredMSG != None
            AlreadyRegisteredMSG.Show()
        EndIf
        Return
    EndIf
    If linkedGame.placedTokenSlot != None && linkedGame.placedTokenSlot != Self
        If SlotFullMSG != None
            SlotFullMSG.Show()
        EndIf
        Return
    EndIf
    If linkedGame.myPlayer != None
        If GameFullMSG != None
            GameFullMSG.Show()
        EndIf
        Return
    EndIf
    If linkedGame.gameActive
        If GameBusyMSG != None
            GameBusyMSG.Show()
        EndIf
        Return
    EndIf
    If Caps001 == None || akActionRef.GetItemCount(Caps001) < 1
        If NoCapsMSG != None
            NoCapsMSG.Show()
        EndIf
        Return
    EndIf
    akActionRef.RemoveItem(Caps001, 1, True)
    linkedGame.myPlayer = akActionRef as Actor
    linkedGame.placedTokenSlot = Self
    linkedGame.Activate(akActionRef)
EndEvent
