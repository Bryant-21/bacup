ScriptName MGRApprenticeQGScript Extends ReferenceAlias

Quest Property MGRQuest Auto Const

Event OnDeath(Actor akKiller)
    MGRQuest.SetStage(255)
EndEvent
