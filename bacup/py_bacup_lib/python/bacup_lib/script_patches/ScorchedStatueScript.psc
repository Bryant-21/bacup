Function BreakStatue()
    If GetState() == "done"
        Return
    EndIf
    GoToState("done")
    DamageObject(1000000.0)
EndFunction

Event OnInit()
    If ScorchedStatueSpawnChance != None
        If Utility.RandomFloat(0.0, 100.0) > ScorchedStatueSpawnChance.GetValue()
            GoToState("done")
            Disable()
            Return
        EndIf
    EndIf
    GoToState("idle")
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If GetState() != "idle"
        Return
    EndIf
    Actor activatingActor = akActionRef as Actor
    If activatingActor != None && LLD_Scorched_Statue != None
        activatingActor.AddItem(LLD_Scorched_Statue)
    EndIf
    BreakStatue()
EndEvent

Event OnHit(ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    BreakStatue()
EndEvent

Event OnTriggerEnter(ObjectReference akActionRef)
    If GetState() != "idle"
        Return
    EndIf
    Float minimumDelay = 0.0
    Float maximumDelay = 0.0
    If ScorchedStatueProxyBreakTimeMin != None
        minimumDelay = ScorchedStatueProxyBreakTimeMin.GetValue()
    EndIf
    If ScorchedStatueProxyBreakTimeMax != None
        maximumDelay = ScorchedStatueProxyBreakTimeMax.GetValue()
    EndIf
    If maximumDelay < minimumDelay
        maximumDelay = minimumDelay
    EndIf
    CancelTimer(1)
    StartTimer(Utility.RandomFloat(minimumDelay, maximumDelay), 1)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        BreakStatue()
    EndIf
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If aiCurrentStage > 0
        GoToState("done")
    EndIf
EndEvent
