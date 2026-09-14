; Original method fill for the hollow Vault 76 muzak scene fragment.
; Phases 1-8 of the scene are gated on GetVMQuestVariable(::CurrentSong_var)
; matching 1..8, so the fragment on the leading ident phase must choose the
; value before those phases evaluate their start conditions.

Bool Function PlayedRecently(SQRadio76QuestScript radioQuest, Int song)
    Return song == radioQuest.LastSong01 || song == radioQuest.LastSong02 \
        || song == radioQuest.LastSong03 || song == radioQuest.LastSong04
EndFunction

Function Fragment_Phase_01_Begin()
    SQRadio76QuestScript radioQuest = \
        Game.GetFormFromFile(0x004F7AF6, "SeventySix.esm") as SQRadio76QuestScript
    If radioQuest == None
        Return
    EndIf

    ; Eight tracks against four history slots always leaves a valid pick; the
    ; attempt cap only bounds the loop.
    Int nextSong = Utility.RandomInt(1, 8)
    Int attempts = 0
    While attempts < 12 && PlayedRecently(radioQuest, nextSong)
        nextSong = Utility.RandomInt(1, 8)
        attempts += 1
    EndWhile

    radioQuest.CurrentSong = nextSong
EndFunction

Function Fragment_Phase_10_End()
    SQRadio76QuestScript radioQuest = \
        Game.GetFormFromFile(0x004F7AF6, "SeventySix.esm") as SQRadio76QuestScript
    If radioQuest != None
        radioQuest.UpdateRadio()
    EndIf
EndFunction
