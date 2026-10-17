Event OnEquipped(Actor akActor)
    If akActor == Game.GetPlayer() && FireOnce == 0
        GoToState("busy")
        Int button = MTRz05_MapWarningMessage.Show()
        ; One dig at a time: while a run is active the map stays unused in the inventory.
        If button == iMessageButtonYesIndex && MTRZ05_Lucky != None && MTRZ05MapKeyword != None && !MTRZ05_Lucky.IsRunning()
            akActor.SetValue(MapValue, iRewardValue as Float)
            MTRZ05MapKeyword.SendStoryEventAndWait(akRef1 = akActor, akRef2 = Self, aiValue1 = iRewardValue)
            If MTRZ05_Lucky.IsRunning()
                FireOnce = 1
                akActor.RemoveItem(GetBaseObject(), 1, True)
            Else
                akActor.SetValue(MapValue, 0.0)
            EndIf
        EndIf
        GoToState("ready")
    EndIf
EndEvent

State busy
    Event OnEquipped(Actor akActor)
    EndEvent
EndState
