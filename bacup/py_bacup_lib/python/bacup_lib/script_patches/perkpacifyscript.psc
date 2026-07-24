Event OnEntryRun(int auiEntryID, ObjectReference akTarget, Actor akOwner)
    If auiEntryID != PacifyEntryID || !akTarget || !akOwner
        Return
    EndIf

    Actor targetActor = akTarget as Actor
    If !targetActor || targetActor == akOwner
        Return
    EndIf
    If akOwner.HasMagicEffect(PacifyCooldownEffect) || targetActor.HasMagicEffect(PacifyEffect)
        Return
    EndIf

    ; Walk from the highest rank down so the actor's best-owned rank wins.
    Int i = PacifyPerksArray.Length - 1
    While i >= 0
        If PacifyPerksArray[i].PacifyPerk && akOwner.HasPerk(PacifyPerksArray[i].PacifyPerk)
            Float roll = Utility.RandomFloat(0.0, 1.0)
            If roll <= PacifyPerksArray[i].chanceToPacify
                If PacifySpell
                    PacifySpell.Cast(akOwner, targetActor)
                EndIf
                If PacifyCooldownSpell
                    PacifyCooldownSpell.Cast(akOwner, akOwner)
                EndIf
            ElseIf PacifyPerksArray[i].chanceToIncite > 0.0 && roll <= (PacifyPerksArray[i].chanceToPacify + PacifyPerksArray[i].chanceToIncite)
                targetActor.StartCombat(akOwner)
            EndIf
            i = -1
        Else
            i -= 1
        EndIf
    EndWhile
EndEvent
