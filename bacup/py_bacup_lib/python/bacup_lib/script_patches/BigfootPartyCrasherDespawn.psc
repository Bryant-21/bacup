; Bigfoot crashes an event: on first load it bursts in (intro explosion and roar), stays non-hostile for
; PresentationDelay seconds, warns halfway through FightDuration and again DespawnMessageBuffer seconds before the
; end, then stomps off back into the forest unless it was killed first.
Event OnLoad()
    If IsDead() || FadeoutTimerDuration > 0.0
        Return
    EndIf
    FadeoutTimerDuration = 300.0
    If FightDuration != None && FightDuration.GetValue() > 0.0
        FadeoutTimerDuration = FightDuration.GetValue()
    EndIf
    HalfwayFightDuration = FadeoutTimerDuration / 2.0
    WarningTimerDuration = Math.Max(1.0, FadeoutTimerDuration - DespawnMessageBuffer)
    If RA_BigfootIntroExplosion != None
        PlaceAtMe(RA_BigfootIntroExplosion)
    EndIf
    If NPCBigfootIntroRoar != None
        NPCBigfootIntroRoar.Play(Self)
    EndIf
    If NonHostileFaction != None && PresentationDelay > 0
        AddToFaction(NonHostileFaction)
        StartTimer(PresentationDelay as Float, 0)
    EndIf
    StartTimer(HalfwayFightDuration, 1)
    StartTimer(WarningTimerDuration, 2)
    StartTimer(FadeoutTimerDuration, 3)
EndEvent

Event OnTimer(Int aiTimerID)
    If IsDead()
        Return
    EndIf
    Actor player = Game.GetPlayer()
    Bool playerNearby = GetDistance(player) <= NearbyPlayerRange
    If aiTimerID == 0
        RemoveFromFaction(NonHostileFaction)
        EvaluatePackage()
    ElseIf aiTimerID == 1
        If playerNearby && BigfootHalfwayMessage != None
            BigfootHalfwayMessage.Show()
        EndIf
    ElseIf aiTimerID == 2
        If playerNearby && BigfootDespawnMessage != None
            BigfootDespawnMessage.Show()
        EndIf
    ElseIf aiTimerID == 3
        Despawn()
    EndIf
EndEvent

Function Despawn()
    If NPCBigfootOutroRoar != None
        NPCBigfootOutroRoar.Play(Self)
    EndIf
    If DespawnAnim != None && Is3DLoaded()
        PlayIdle(DespawnAnim)
        Utility.Wait(FadeoutTimeDelay as Float)
    EndIf
    If IsDead()
        Return
    EndIf
    If DespawnExplosion != None
        PlaceAtMe(DespawnExplosion)
    EndIf
    Disable()
    Delete()
EndFunction
