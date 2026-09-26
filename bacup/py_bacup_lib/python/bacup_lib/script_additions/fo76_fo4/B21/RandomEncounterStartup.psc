Scriptname B21:RandomEncounterStartup Extends Quest

; FO76's random encounters bind Fallout 4's own REScript, but the stage
; fragment that calls Startup() was stripped server-side, so nothing ever
; runs it. Startup() is what registers the quest for REParent's
; RECheckForCleanup event, so without it a converted encounter never stops:
; its actors persist, REParent's running-encounter budget fills permanently,
; and SendStoryEventAndWait then fails for every later encounter.

Event OnQuestInit()
	Quest OwningQuest = Self As Quest
	REScript Encounter = OwningQuest As REScript
	If Encounter
		Encounter.Startup()
	EndIf
EndEvent
