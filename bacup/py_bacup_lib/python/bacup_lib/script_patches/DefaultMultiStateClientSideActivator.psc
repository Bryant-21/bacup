Function ClientPlayAnimation(String animationStateName)
    If AnimationStates == None || AnimationStates.Length == 0
        Return
    EndIf

    Int animationIndex = 0
    If animationStateName != ""
        animationIndex = -1
        Int i = 0
        While i < AnimationStates.Length
            If AnimationStates[i].StateName == animationStateName
                animationIndex = i
                i = AnimationStates.Length
            Else
                i += 1
            EndIf
        EndWhile
    EndIf

    If animationIndex < 0 || !Is3DLoaded()
        Return
    EndIf

    If AnimationStates[animationIndex].StateStartAnim != ""
        PlayAnimation(AnimationStates[animationIndex].StateStartAnim)
    Else
        PlayAnimation(AnimationStates[animationIndex].StateJumpAnim)
    EndIf
EndFunction
