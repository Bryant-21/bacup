; The Blood Eagle spotter dog barks to raise the alarm when it starts searching or fighting.
Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
    If aeCombatState != 0 && BarkingSound != None && !IsDead()
        BarkingSound.Play(Self)
    EndIf
EndEvent
