; Thomas farm clue count for objective 10 ("Search for clues (CluesFound/MaxClues)"): the five search clues are the
; stages gated on 100 and turned off by 675 (note 250, barrel 300, jar 400, aerosolizer 500, note 600). Finding all
; of them closes the search (675).
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID != 250 && auiStageID != 300 && auiStageID != 400 && auiStageID != 500 && auiStageID != 600
        Return
    EndIf
    CluesFound = CountFoundClues()
    Actor player = Game.GetPlayer()
    If P01B_Mini_Albino01_CluesFound != None
        player.SetValue(P01B_Mini_Albino01_CluesFound, CluesFound as Float)
    EndIf
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("CluesFound", CluesFound as Float)
        questVariables.SetVariable("MaxClues", MaxClues as Float)
    EndIf
    If MaxClues > 0 && CluesFound >= MaxClues && !IsStageDone(675)
        SetStage(675)
    EndIf
EndEvent

Int Function CountFoundClues()
    Int found = 0
    If IsStageDone(250)
        found += 1
    EndIf
    If IsStageDone(300)
        found += 1
    EndIf
    If IsStageDone(400)
        found += 1
    EndIf
    If IsStageDone(500)
        found += 1
    EndIf
    If IsStageDone(600)
        found += 1
    EndIf
    Return found
EndFunction
