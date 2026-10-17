; Cars and barrels score once when destroyed; LCP_Explosions scales them by ExplosionsCompensateMultiplier.
Function ResetRef(ObjectReference akRef)
    Parent.ResetRef(akRef)
    akRef.ClearDestruction()
EndFunction

Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
EndEvent

Event OnDestructionStageChanged(ObjectReference akSenderRef, Int aiOldStage, Int aiCurrentStage)
    If akSenderRef == None || !akSenderRef.IsDestroyed() || !MischiefOpen() || !ClaimRef(akSenderRef)
        Return
    EndIf
    ScoreMischief(akSenderRef, Game.GetPlayer(), (GetOwningQuest() as MN2_QuestScript).ExplosionMultiplier())
EndEvent
