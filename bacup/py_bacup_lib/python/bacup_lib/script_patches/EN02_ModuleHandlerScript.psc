Event OnQuestInit()
    BeginModuleSequence()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iIntialTimerID
        TriggerModuleBlast()
        StartTimer(iShutdownTimerLength as Float, iShutdownTimerID)
    ElseIf aiTimerID == iShutdownTimerID
        FinishModuleSequence()
    EndIf
EndEvent

Function BeginModuleSequence()
    ObjectReference moduleRef = Module.GetRef()
    ObjectReference enableRef = EnableMarker.GetRef()
    If moduleRef != None
        moduleRef.Enable(False)
        If QSTEN02OverrideModuleActivate != None
            QSTEN02OverrideModuleActivate.Play(moduleRef)
        EndIf
    EndIf
    If enableRef != None
        enableRef.Enable(False)
    EndIf
    StartTimer(iInitialTimerLength as Float, iIntialTimerID)
EndFunction

Function TriggerModuleBlast()
    ObjectReference fxRef = FXSpawn.GetRef()
    Actor playerActor = PlayerRef.GetActorRef()
    If fxRef != None && TargetExplosion != None
        fxRef.PlaceAtMe(TargetExplosion, 1, False, False)
    EndIf
    If playerActor != None && EN02_RadarBlastSpell01 != None
        EN02_RadarBlastSpell01.Cast(playerActor, playerActor)
    EndIf
    If QSTEN02RadioTransmit2D != None
        QSTEN02RadioTransmit2D.Play(fxRef)
    EndIf
EndFunction

Function FinishModuleSequence()
    ObjectReference fxRef = FXSpawn.GetRef()
    If fxRef != None
        fxRef.Disable(False)
    EndIf
    Stop()
EndFunction
