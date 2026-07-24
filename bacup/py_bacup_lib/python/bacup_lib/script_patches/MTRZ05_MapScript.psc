Event OnEquipped(Actor akActor)
    If akActor == Game.GetPlayer() && FireOnce == 0
        GoToState("busy")
        Int button = MTRz05_MapWarningMessage.Show()
        If button == iMessageButtonYesIndex
            akActor.SetValue(MapValue, iRewardValue as Float)
            FireOnce = 1
        EndIf
        GoToState("ready")
    EndIf
EndEvent
