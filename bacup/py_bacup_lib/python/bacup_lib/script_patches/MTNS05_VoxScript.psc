Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget == None || akCaster != Game.GetPlayer()
        Return
    EndIf

    MTNS05QuestScript voicesQuest = MTNS05_Voices as MTNS05QuestScript
    If voicesQuest != None
        voicesQuest.HandleVoxTarget(akTarget, self)
    EndIf
EndEvent
