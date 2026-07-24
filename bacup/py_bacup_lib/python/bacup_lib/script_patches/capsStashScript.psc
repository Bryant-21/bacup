Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    GoToState("busy")

    Float roll = Utility.RandomFloat(0.0, 1.0)
    Int amount
    If roll < capsJackpotChance
        amount = Utility.RandomInt(capsJackpotMin, capsJackpotMax)
    ElseIf roll < capsJackpotChance + capsHighChance
        amount = Utility.RandomInt(capsHighMin, capsHighMax)
    ElseIf roll < capsJackpotChance + capsHighChance + capsMediumChance
        amount = Utility.RandomInt(capsMediumMin, capsMediumMax)
    Else
        amount = Utility.RandomInt(capsStandardMin, capsStandardMax)
    EndIf

    Float bonusChance = 0.0
    Float bonusMod = 1.0
    If akActionRef.HasPerk(CapCollector03)
        bonusChance = capCollectorChanceRank3
        bonusMod = capCollectorCapsModRank3
    ElseIf akActionRef.HasPerk(CapCollector02)
        bonusChance = capCollectorChanceRank2
        bonusMod = capCollectorCapsModRank2
    ElseIf akActionRef.HasPerk(CapCollector01)
        bonusChance = capCollectorChanceRank1
        bonusMod = capCollectorCapsModRank1
    EndIf

    If bonusChance > 0.0
        If akActionRef.HasPerk(CapsBobbleheadPerk)
            bonusChance = bonusChance * capsBobbleheadChanceMultiplier
            If bonusChance > 1.0
                bonusChance = 1.0
            EndIf
        EndIf
        If Utility.RandomFloat(0.0, 1.0) < bonusChance
            amount = (amount as Float * bonusMod) as Int
        EndIf
    EndIf

    akActionRef.AddItem(Caps001, amount)
    ShowVaultboy()
    Disable()
EndEvent
