Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
    If aeCombatState == 1 && LinkedDeathclaws.Length == 0
        If NoStaggerAll
            AddKeyword(NoStaggerAll)
        EndIf
        If Deathclaw_Roar
            PlayIdle(Deathclaw_Roar)
        EndIf
        SpawnDeathclaws()
    EndIf
EndEvent

Function SpawnDeathclaws()
    If !Burn_LvlDeathclaw_Armored
        Return
    EndIf

    Int i = 0
    While i < NumberOfDeathclawsToSpawn
        Float angle = Utility.RandomFloat(0.0, 360.0)
        Float dist = Utility.RandomFloat(MinDeathclawSpawnRadius, MaxDeathclawSpawnRadius)
        Actor newDeathclaw = PlaceActorAtMe(Burn_LvlDeathclaw_Armored, AiLevelForDeathclaw)
        If newDeathclaw
            newDeathclaw.MoveTo(Self, dist * Math.cos(angle), dist * Math.sin(angle), 0.0)
            If Burn_RustRaiderFaction
                newDeathclaw.AddToFaction(Burn_RustRaiderFaction)
            EndIf
            If Aggression
                newDeathclaw.SetValue(Aggression, Frenzied)
            EndIf
            LinkedDeathclaws.Add(newDeathclaw)
        EndIf
        i += 1
    EndWhile
EndFunction
