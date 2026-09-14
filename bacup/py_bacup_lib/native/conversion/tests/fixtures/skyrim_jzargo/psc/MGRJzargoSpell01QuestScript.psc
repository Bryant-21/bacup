ScriptName MGRJzargoSpell01QuestScript Extends Quest Conditional

GlobalVariable Property MGRJ1Total Auto Const
GlobalVariable Property MGRJ1Test Auto Const
Int Property HelpAsked Auto
Int Property EffectTriggered Auto

Function VCount()
    ModObjectiveGlobal(1.0, MGRJ1Test, 20, -1.0, True, True, True)
    If MGRJ1Test.Value == MGRJ1Total.Value
        SetStage(30)
    EndIf
EndFunction
