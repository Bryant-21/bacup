Actor Function BossReference()
    Return GetActorReference()
EndFunction

Function PrepareForFight()
    Actor boss = BossReference()
    If boss == None
        Return
    EndIf
    boss.Enable()
    boss.RemoveFromFaction(NonCombatFaction)
    boss.RemovePerk(JerseyDevilPacifyPerk)
    boss.AddKeyword(JerseyDevilCannotBePacified)
    If Aggression != None
        boss.SetValue(Aggression, 2.0)
    EndIf
    boss.SetGhost(False)
    boss.EvaluatePackage()
    GoToState("jerseydevilcombat")
EndFunction

Function EnterDownedState()
    Actor boss = BossReference()
    If boss == None || GetState() == "jerseydevildown" || GetState() == "jerseydevilexit"
        Return
    EndIf
    boss.StopCombat()
    boss.AddToFaction(NonCombatFaction)
    boss.AddPerk(JerseyDevilPacifyPerk)
    boss.RemoveKeyword(JerseyDevilCannotBePacified)
    If Aggression != None
        boss.SetValue(Aggression, 0.0)
    EndIf
    If JerseyDevilHealth != None
        boss.SetValue(JerseyDevilHealth, MinJerseyDevilHealthPercentage)
    EndIf
    If JerseyDevilStaggerSpell != None
        JerseyDevilStaggerSpell.Cast(boss, boss)
    EndIf
    RegisterForAnimationEvent(boss, AnimDoneEvent)
    boss.PlayIdle(JerseyDevil_BleedOut_Enter)
    GoToState("jerseydevildown")
    If AC_MQ04_Sins != None && !AC_MQ04_Sins.IsStageDone(180)
        AC_MQ04_Sins.SetStage(180)
    EndIf
EndFunction

Function ReleaseAfterHarvest()
    Actor boss = BossReference()
    If boss == None || GetState() == "jerseydevilexit"
        Return
    EndIf
    boss.RemovePerk(JerseyDevilPacifyPerk)
    boss.AddKeyword(JerseyDevilCannotBePacified)
    RegisterForAnimationEvent(boss, AnimDoneEvent)
    boss.PlayIdle(JerseyDevil_BleedOut_Exit)
    GoToState("jerseydevilexit")
EndFunction

Function CleanupBoss()
    Actor boss = BossReference()
    If boss == None
        Return
    EndIf
    UnregisterForAnimationEvent(boss, AnimDoneEvent)
    boss.StopCombat()
    boss.RemovePerk(JerseyDevilPacifyPerk)
    boss.RemoveKeyword(JerseyDevilCannotBePacified)
    boss.AddToFaction(NonCombatFaction)
    If Aggression != None
        boss.SetValue(Aggression, 0.0)
    EndIf
EndFunction

Event OnAliasInit()
    Actor boss = BossReference()
    If boss != None
        RegisterForAnimationEvent(boss, AnimDoneEvent)
    EndIf
EndEvent

Event OnEnterBleedout()
    EnterDownedState()
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
    Actor boss = BossReference()
    If boss == None || akSource != boss || asEventName != AnimDoneEvent
        Return
    EndIf
    If GetState() == "jerseydevildown"
        boss.PlayIdle(JerseyDevil_BleedOut_Loop)
        GoToState("jerseydevilloop")
    ElseIf GetState() == "jerseydevilexit"
        If JerseyDevilExitVFX != None
            boss.PlaceAtMe(JerseyDevilExitVFX)
        EndIf
        boss.Disable()
        CleanupBoss()
    EndIf
EndEvent
