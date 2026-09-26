; Single-player lunchbox reward. The converted reward effect is conditioned to
; the player, so FO76 team sharing does not apply. Favor spells are timed
; fire-and-forget spells, so they are cast rather than added as abilities.
; PartyFavorFanfarePerk is not granted: FO4 has no perk-fanfare HUD, and the
; Confetti, So Much Cake and To-Go Punch entries bind the same perks their favor
; effects already apply through PerkToApply, so adding them would make those
; timed favors permanent.

Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget == None || akTarget != Game.GetPlayer()
        Return
    EndIf

    If SCORE_Lunchbox_XP_Effect
        SCORE_Lunchbox_XP_Effect.Cast(akTarget, akTarget)
    EndIf
    If GuarenteedBuffSpell
        GuarenteedBuffSpell.Cast(akTarget, akTarget)
    EndIf

    If LunchboxPartyFavors == None || LunchboxPartyFavors.Length == 0
        Return
    EndIf
    PartyFavorData favor = LunchboxPartyFavors[Utility.RandomInt(0, LunchboxPartyFavors.Length - 1)]
    If favor == None
        Return
    EndIf
    If favor.PartyFavorSpell
        favor.PartyFavorSpell.Cast(akTarget, akTarget)
    EndIf
    If favor.PartyFavorMessage
        favor.PartyFavorMessage.Show()
    EndIf
EndEvent
