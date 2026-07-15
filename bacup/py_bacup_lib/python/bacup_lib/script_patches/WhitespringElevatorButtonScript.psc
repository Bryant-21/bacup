Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    ObjectReference elevatorRef = GetLinkedRef()
    If elevatorRef == None || !elevatorRef.IsEnabled()
        WhitespringElevatorButtonInactiveMessage.Show()
        Return
    EndIf

    OBJLoadElevatorUtilityButtonPanel.Play(Self)
    elevatorRef.Activate(akActionRef)
EndEvent
