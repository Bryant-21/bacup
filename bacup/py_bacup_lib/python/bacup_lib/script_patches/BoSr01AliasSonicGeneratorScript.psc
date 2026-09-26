Event OnLoad()
    Parent.OnLoad()
    BoSr01_Script eventScript = GetOwningQuest() as BoSr01_Script
    If eventScript != None
        eventScript.PrepareGenerator()
        eventScript.UpdateGeneratorState()
    EndIf
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    Parent.OnDestructionStageChanged(aiOldStage, aiCurrentStage)
    BoSr01_Script eventScript = GetOwningQuest() as BoSr01_Script
    If eventScript != None
        eventScript.UpdateGeneratorState()
    EndIf
EndEvent
