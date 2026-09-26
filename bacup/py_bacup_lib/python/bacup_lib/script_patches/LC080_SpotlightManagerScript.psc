Function ToggleSpotlightLightSystemClient(Bool shouldEnable, Bool shouldPlaySFX)
    If shouldPlaySFX
        If shouldEnable && OBJLightsPowerOn != None
            OBJLightsPowerOn.Play(Self)
        ElseIf !shouldEnable && OBJLightsPowerOff != None
            OBJLightsPowerOff.Play(Self)
        EndIf
    EndIf
    ObjectReference[] spotlights = GetLinkedRefChain(None, 100)
    Int index = 0
    While index < spotlights.Length
        ObjectReference spotlight = spotlights[index]
        If spotlight != None
            If spotlight.Is3DLoaded()
                If shouldEnable
                    spotlight.PlayAnimation("TurnOn")
                    spotlight.PlayAnimation("Direct01")
                    spotlight.SetDirectAtTarget(Game.GetPlayer())
                Else
                    spotlight.PlayAnimation("TurnOff")
                    spotlight.SetDirectAtTarget(GetLinkedRef())
                EndIf
            EndIf
            If LinkSpotlightLight != None
                ObjectReference[] lights = spotlight.GetLinkedRefChain(LinkSpotlightLight, 100)
                Int lightIndex = 0
                While lightIndex < lights.Length
                    ObjectReference lightRef = lights[lightIndex]
                    If lightRef != None
                        If shouldEnable
                            lightRef.EnableNoWait()
                        Else
                            lightRef.DisableNoWait()
                        EndIf
                    EndIf
                    lightIndex += 1
                EndWhile
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Bool Function IsLocalSpotlightEnabled()
    Actor player = Game.GetPlayer()
    Return player != None && LC080_SpotlightTriggerActorValue != None \
        && player.GetValue(LC080_SpotlightTriggerActorValue) == LC080_SpotlightTriggerActorValueValue
EndFunction

Function SetLocalSpotlightEnabled(Bool abEnabled)
    Actor player = Game.GetPlayer()
    If player == None || LC080_SpotlightTriggerActorValue == None
        Return
    EndIf
    Bool changed = IsLocalSpotlightEnabled() != abEnabled
    If abEnabled
        player.SetValue(LC080_SpotlightTriggerActorValue, LC080_SpotlightTriggerActorValueValue as Float)
    Else
        player.SetValue(LC080_SpotlightTriggerActorValue, 0.0)
    EndIf
    If changed
        ToggleSpotlightLightSystemClient(abEnabled, True)
    EndIf
EndFunction

Function ReconcileSpotlightOccupancy()
    If linkedTriggers == None || linkedTriggers.Length == 0
        Return
    EndIf
    Bool occupied = False
    Int index = 0
    While index < linkedTriggers.Length
        If linkedTriggers[index] != None && linkedTriggers[index].GetTriggerObjectCount() > 0
            occupied = True
        EndIf
        index += 1
    EndWhile
    SetLocalSpotlightEnabled(occupied)
EndFunction

Event OnLoad()
    If LinkTrigger != None
        linkedTriggers = GetLinkedRefChain(LinkTrigger, 100)
        Int index = 0
        While index < linkedTriggers.Length
            If linkedTriggers[index] != None
                RegisterForRemoteEvent(linkedTriggers[index], "OnTriggerEnter")
                RegisterForRemoteEvent(linkedTriggers[index], "OnTriggerLeave")
            EndIf
            index += 1
        EndWhile
    EndIf
    ObjectReference[] spotlights = GetLinkedRefChain(None, 100)
    Int spotlightIndex = 0
    While spotlightIndex < spotlights.Length
        If spotlights[spotlightIndex] != None
            RegisterForRemoteEvent(spotlights[spotlightIndex], "OnLoad")
        EndIf
        spotlightIndex += 1
    EndWhile
    ToggleSpotlightLightSystemClient(IsLocalSpotlightEnabled(), False)
    StartTimer(CONST_SpotlightTriggerTimerDelay as Float, CONST_SpotlightTriggerTimerID)
EndEvent

Event ObjectReference.OnLoad(ObjectReference akSender)
    If Is3DLoaded()
        ToggleSpotlightLightSystemClient(IsLocalSpotlightEnabled(), False)
    EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        SetLocalSpotlightEnabled(True)
    EndIf
EndEvent

Event ObjectReference.OnTriggerLeave(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        ReconcileSpotlightOccupancy()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == CONST_SpotlightTriggerTimerID && Is3DLoaded()
        ReconcileSpotlightOccupancy()
        StartTimer(CONST_SpotlightTriggerTimerDelay as Float, CONST_SpotlightTriggerTimerID)
    EndIf
EndEvent

Event OnUnload()
    CancelTimer(CONST_SpotlightTriggerTimerID)
    UnregisterForAllRemoteEvents()
EndEvent
