; A shielded siphon shrugs off damage: its destruction is cleared until the controller drops the shield.
Function SetShield(ObjectReference akSiphon, Bool abShielded)
    If akSiphon == None || ForceFieldEffectShader == None
        Return
    EndIf
    If abShielded
        ForceFieldEffectShader.Play(akSiphon)
    Else
        ForceFieldEffectShader.Stop(akSiphon)
        If WL006_TurretOverridePowerUp != None
            WL006_TurretOverridePowerUp.Play(akSiphon)
        EndIf
    EndIf
EndFunction

Function Pulse(ObjectReference akSiphon)
    If akSiphon != None && E07B_Invaders_Explosion_ExtractorPulse != None
        akSiphon.PlaceAtMe(E07B_Invaders_Explosion_ExtractorPulse)
    EndIf
EndFunction

Event OnDestructionStageChanged(ObjectReference akSenderRef, Int aiOldStage, Int aiCurrentStage)
    OwningQuest = GetOwningQuest()
    QuestScript = OwningQuest as Quests:E07B_Invaders:QuestScript
    If akSenderRef != None && QuestScript != None && QuestScript.IsShielded(akSenderRef)
        akSenderRef.ClearDestruction()
    EndIf
EndEvent
