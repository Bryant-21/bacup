Event OnAliasInit()
    If TriggeringAlias != None && TriggeringAlias.GetReference() != None
        RegisterForRemoteEvent(TriggeringAlias.GetReference(), "OnTriggerEnter")
    EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If W05_Daniel_ToggleTriggerApproach == None || W05_Daniel_ToggleTriggerApproach.GetValue() <= 0.0
        Return
    EndIf
    If SceneToPlay == None
        Return
    EndIf
    If SceneToPlay.IsPlaying()
        Return
    EndIf
    SceneToPlay.Start()
EndEvent
