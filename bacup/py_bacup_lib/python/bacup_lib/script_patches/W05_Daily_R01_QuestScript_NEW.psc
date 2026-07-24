Event OnQuestInit()
    InitializeTechSelection()
EndEvent

Function InitializeTechSelection()
    Int wanted = TechTotal
    If wanted <= 0
        wanted = 3
    EndIf
    Int broken = TechBroken
    If broken <= 0
        broken = 1
    EndIf
    If broken > wanted
        broken = wanted
    EndIf

    Int poolSize = NormalTechStages.Length
    If poolSize <= 0
        Return
    EndIf
    If wanted > poolSize
        wanted = poolSize
    EndIf

    Bool[] used = new Bool[poolSize]
    Int selected = 0
    Int brokenAssigned = 0
    While selected < wanted
        Int pick = Utility.RandomInt(0, poolSize - 1)
        If !used[pick]
            used[pick] = True
            Bool makeBroken = False
            If brokenAssigned < broken
                makeBroken = True
            EndIf
            If makeBroken
                SetStage(BrokenTechStages[pick])
                brokenAssigned += 1
            Else
                SetStage(NormalTechStages[pick])
            EndIf
            selected += 1
        EndIf
    EndWhile
EndFunction
