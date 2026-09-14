; Johnny's reference alias in W05_MQA_206P ("Secrets Revealed").  The only bound
; property is the quest's player alias, and the only stage in the Johnny
; confrontation branch that nothing else reaches is 5220 ("Player attacks
; Johnny"), whose fragment displays the "Kill Johnny" objective.  Johnny
; entering combat with the player during the confrontation is that transition.
;
; Stage 5275 already starts combat itself and 5240/5250/5300 close the branch,
; so those are excluded to keep the transition one-way.

Event OnCombatStateChanged(Actor akTarget, int aeCombatState)
    If aeCombatState == 0 || PlayerAlias == None
        Return
    EndIf
    Actor playerRef = PlayerAlias.GetActorReference()
    If playerRef == None || akTarget != playerRef
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None
        Return
    EndIf
    If !owningQuest.IsStageDone(5200)
        Return
    EndIf
    If owningQuest.IsStageDone(5220) || owningQuest.IsStageDone(5240) || owningQuest.IsStageDone(5250)
        Return
    EndIf
    If owningQuest.IsStageDone(5275) || owningQuest.IsStageDone(5300)
        Return
    EndIf
    owningQuest.SetStage(5220)
EndEvent
