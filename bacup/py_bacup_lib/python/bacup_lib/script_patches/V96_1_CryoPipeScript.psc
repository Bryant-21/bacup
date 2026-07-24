Function SetAnimationStateCryoPipe(Int aiNewIndex)
    While lock_SetAnimationStateCryoPipe
        Utility.Wait(0.1)
    EndWhile
    lock_SetAnimationStateCryoPipe = True

    If aiNewIndex < 0 || aiNewIndex >= AnimationStates.Length
        lock_SetAnimationStateCryoPipe = False
        Return
    EndIf

    CurrentStateIndex = aiNewIndex
    If AnimationStates[aiNewIndex].StateAdditionalAnim != ""
        PlayAnimation(AnimationStates[aiNewIndex].StateAdditionalAnim)
    EndIf
    If AnimationStates[aiNewIndex].StateStartAnim != ""
        PlayAnimation(AnimationStates[aiNewIndex].StateStartAnim)
    Else
        PlayAnimation(AnimationStates[aiNewIndex].StateJumpAnim)
    EndIf
    If AnimationStates[aiNewIndex].EnableBlockActivation
        BlockActivation(AnimationStates[aiNewIndex].BlockActivation, AnimationStates[aiNewIndex].BlockActivationHideText)
    EndIf
    If AnimationStates[aiNewIndex].EnableOverrideActivateText
        SetActivateTextOverride(AnimationStates[aiNewIndex].OverrideActivateText)
    EndIf

    lock_SetAnimationStateCryoPipe = False
EndFunction

Event OnInit()
    SetAnimationStateCryoPipe(0)
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If CryoPipeIsDestroyed
        Return
    EndIf

    CryoPipeIsDestroyed = True
    If CryoPipeExplosion
        PlaceAtMe(CryoPipeExplosion)
    EndIf
    SetDestroyed(True)
EndEvent
