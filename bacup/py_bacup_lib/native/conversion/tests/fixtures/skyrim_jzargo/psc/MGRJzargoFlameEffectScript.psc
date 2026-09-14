ScriptName MGRJzargoFlameEffectScript Extends ActiveMagicEffect

Explosion Property FireballExplosion Auto Const
Quest Property Jzargo01 Auto Const
GlobalVariable Property TestCast Auto Const
Keyword Property UndeadKeyword Auto Const

Event OnEffectStart(Actor akTarget, Actor akCaster)
    MGRJzargoSpell01QuestScript questScript = Jzargo01 as MGRJzargoSpell01QuestScript
    If akTarget.HasKeyword(UndeadKeyword)
        akTarget.PlaceAtMe(FireballExplosion, 1, False, False)
        If Jzargo01.GetStage() == 20
            questScript.VCount()
        EndIf
    EndIf
EndEvent
