Event OnEquipped(Actor akActor)
    If akActor == Game.GetPlayer() && FireOnce == 0
        GoToState("busy")
        Int button = MTRz05_MapWarningMessage.Show()
        If button == iMessageButtonYesIndex
            If MTRZ05_Lucky != None && (MTRZ05_Lucky.IsRunning() || MTRZ05_Lucky.IsCompleted())
                FireOnce = 1
            ElseIf MTRZ05_Lucky != None && MTRZ05MapKeyword != None
                akActor.SetValue(MapValue, iRewardValue as Float)
                MTRZ05MapKeyword.SendStoryEventAndWait(akRef1 = akActor, akRef2 = Self, aiValue1 = iRewardValue)
                If MTRZ05_Lucky.IsRunning() || MTRZ05_Lucky.IsCompleted()
                    FireOnce = 1
                    akActor.RemoveItem(Self, 1, True)
                EndIf
            EndIf
        EndIf
        GoToState("ready")
    EndIf
EndEvent

State busy
    Event OnEquipped(Actor akActor)
    EndEvent
EndState
